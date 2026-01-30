use std::collections::BTreeSet;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{Class, Method};
use crate::rules::{Rule, RuleMetadata, class_location, method_location_with_line, result_message};

const TARGET_COLLECTION_TYPES: [&str; 8] = [
    "java/util/Set",
    "java/util/List",
    "java/util/Collection",
    "java/util/HashSet",
    "java/util/LinkedHashSet",
    "java/util/TreeSet",
    "java/util/ArrayList",
    "java/util/LinkedList",
];

/// Rule that flags enum collections that should use EnumSet instead.
#[derive(Default)]
pub(crate) struct PreferEnumSetRule;

crate::register_rule!(PreferEnumSetRule);

impl Rule for PreferEnumSetRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "PREFER_ENUMSET",
            name: "Prefer EnumSet for enum collections",
            description: "Using EnumSet for enum types provides better performance than general collections",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let enums = identify_enum_types(context);
        if enums.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
            let local_variable_entries: usize = class
                .methods
                .iter()
                .map(|method| method.local_variable_types.len())
                .sum();
            let methods_without_local_variable_types = class
                .methods
                .iter()
                .filter(|method| {
                    !method.bytecode.is_empty() && method.local_variable_types.is_empty()
                })
                .count();
            let mut attributes = vec![
                KeyValue::new("inspequte.class", class.name.clone()),
                KeyValue::new(
                    "inspequte.prefer_enumset.local_variable_entries",
                    local_variable_entries as i64,
                ),
                KeyValue::new(
                    "inspequte.prefer_enumset.methods_without_local_variable_types",
                    methods_without_local_variable_types as i64,
                ),
            ];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let artifact_uri = context.class_artifact_uri(class);
            let class_results =
                context.with_span("class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    class_results.extend(check_fields(class, &enums));
                    class_results.extend(check_methods(class, &enums, artifact_uri.as_deref()));
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

#[derive(Clone, Debug)]
/// Parsed class type with optional generic arguments.
struct ClassTypeSignature {
    name: String,
    type_args: Vec<TypeSignature>,
}

#[derive(Clone, Debug)]
/// Parsed method signature containing parameters and return type.
struct MethodSignature {
    params: Vec<TypeSignature>,
    return_type: Option<TypeSignature>,
}

#[derive(Clone, Debug)]
/// Parsed signature value used for generic-aware checks.
#[allow(dead_code)]
enum TypeSignature {
    Base(char),
    Array(Box<TypeSignature>),
    TypeVar(String),
    Class(ClassTypeSignature),
    Wildcard,
    Void,
}

#[derive(Clone, Debug)]
/// Parsed signature match for a collection of enum types.
struct EnumCollectionMatch {
    collection: String,
    enum_name: String,
}

#[derive(Clone, Debug)]
/// Parser for generic signatures as defined by the JVM spec.
struct SignatureParser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> SignatureParser<'a> {
    fn new(signature: &'a str) -> Self {
        Self {
            bytes: signature.as_bytes(),
            offset: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        Some(byte)
    }

    fn skip_type_parameters(&mut self) {
        if self.peek() != Some(b'<') {
            return;
        }
        let mut depth = 0u32;
        while let Some(byte) = self.bump() {
            match byte {
                b'<' => depth += 1,
                b'>' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_method_signature(&mut self) -> Option<MethodSignature> {
        self.skip_type_parameters();
        if self.bump()? != b'(' {
            return None;
        }
        let mut params = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b')' {
                self.bump();
                break;
            }
            params.push(self.parse_type_signature()?);
        }
        let return_type = match self.peek()? {
            b'V' => {
                self.bump();
                None
            }
            _ => Some(self.parse_type_signature()?),
        };
        Some(MethodSignature {
            params,
            return_type,
        })
    }

    fn parse_type_signature(&mut self) -> Option<TypeSignature> {
        match self.peek()? {
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => {
                Some(TypeSignature::Base(self.bump()? as char))
            }
            b'V' => {
                self.bump();
                Some(TypeSignature::Void)
            }
            b'[' => {
                self.bump();
                let component = self.parse_type_signature()?;
                Some(TypeSignature::Array(Box::new(component)))
            }
            b'T' => self.parse_type_variable(),
            b'L' => self.parse_class_type_signature().map(TypeSignature::Class),
            _ => None,
        }
    }

    fn parse_reference_type_signature(&mut self) -> Option<TypeSignature> {
        match self.peek()? {
            b'L' => self.parse_class_type_signature().map(TypeSignature::Class),
            b'T' => self.parse_type_variable(),
            b'[' => {
                self.bump();
                let component = self.parse_type_signature()?;
                Some(TypeSignature::Array(Box::new(component)))
            }
            _ => None,
        }
    }

    fn parse_type_variable(&mut self) -> Option<TypeSignature> {
        if self.bump()? != b'T' {
            return None;
        }
        let mut name = String::new();
        while let Some(byte) = self.peek() {
            if byte == b';' {
                self.bump();
                break;
            }
            name.push(byte as char);
            self.bump();
        }
        Some(TypeSignature::TypeVar(name))
    }

    fn parse_class_type_signature(&mut self) -> Option<ClassTypeSignature> {
        if self.bump()? != b'L' {
            return None;
        }
        let mut name = String::new();
        while let Some(byte) = self.peek() {
            match byte {
                b';' | b'<' | b'.' => break,
                _ => {
                    name.push(byte as char);
                    self.bump();
                }
            }
        }
        if name.is_empty() {
            return None;
        }
        let mut type_args = Vec::new();
        if self.peek() == Some(b'<') {
            type_args = self.parse_type_arguments()?;
        }
        while self.peek() == Some(b'.') {
            self.bump();
            name.push('$');
            while let Some(byte) = self.peek() {
                match byte {
                    b';' | b'<' | b'.' => break,
                    _ => {
                        name.push(byte as char);
                        self.bump();
                    }
                }
            }
            if self.peek() == Some(b'<') {
                self.skip_type_arguments();
            }
        }
        if self.peek() == Some(b';') {
            self.bump();
        }
        Some(ClassTypeSignature { name, type_args })
    }

    fn parse_type_arguments(&mut self) -> Option<Vec<TypeSignature>> {
        if self.bump()? != b'<' {
            return None;
        }
        let mut args = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b'>' {
                self.bump();
                break;
            }
            let arg = match byte {
                b'*' => {
                    self.bump();
                    TypeSignature::Wildcard
                }
                b'+' | b'-' => {
                    self.bump();
                    self.parse_reference_type_signature()?
                }
                _ => self.parse_reference_type_signature()?,
            };
            args.push(arg);
        }
        Some(args)
    }

    fn skip_type_arguments(&mut self) {
        if self.peek() != Some(b'<') {
            return;
        }
        let mut depth = 0u32;
        while let Some(byte) = self.bump() {
            match byte {
                b'<' => depth += 1,
                b'>' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
}

fn identify_enum_types(context: &AnalysisContext) -> BTreeSet<String> {
    context
        .classes
        .iter()
        .filter(|class| class.super_name.as_deref() == Some("java/lang/Enum"))
        .map(|class| class.name.clone())
        .collect()
}

fn check_fields(class: &Class, enums: &BTreeSet<String>) -> Vec<SarifResult> {
    let mut results = Vec::new();
    for field in &class.fields {
        let Some(signature) = field.signature.as_deref() else {
            continue;
        };
        let mut parser = SignatureParser::new(signature);
        let Some(ty) = parser.parse_type_signature() else {
            continue;
        };
        let Some(matched) = match_collection_enum(&ty, enums) else {
            continue;
        };
        let message =
            enum_collection_message(&matched, &format!("field {}.{}", class.name, field.name));
        results.push(
            SarifResult::builder()
                .message(message)
                .locations(vec![class_location(&class.name)])
                .build(),
        );
    }
    results
}

fn check_methods(
    class: &Class,
    enums: &BTreeSet<String>,
    artifact_uri: Option<&str>,
) -> Vec<SarifResult> {
    let mut results = Vec::new();
    for method in &class.methods {
        results.extend(check_method_signature(class, method, enums, artifact_uri));
        results.extend(check_local_variables(class, method, enums, artifact_uri));
    }
    results
}

fn check_method_signature(
    class: &Class,
    method: &Method,
    enums: &BTreeSet<String>,
    artifact_uri: Option<&str>,
) -> Vec<SarifResult> {
    let mut results = Vec::new();
    let Some(signature) = method.signature.as_deref() else {
        return results;
    };
    let mut parser = SignatureParser::new(signature);
    let Some(method_signature) = parser.parse_method_signature() else {
        return results;
    };
    let line = method.line_for_offset(0);
    if let Some(return_type) = method_signature.return_type {
        if let Some(matched) = match_collection_enum(&return_type, enums) {
            let message = enum_collection_message(
                &matched,
                &format!("return type of {}.{}", class.name, method.name),
            );
            let location = method_location_with_line(
                &class.name,
                &method.name,
                &method.descriptor,
                artifact_uri,
                line,
            );
            results.push(
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location])
                    .build(),
            );
        }
    }
    for (index, param) in method_signature.params.iter().enumerate() {
        let Some(matched) = match_collection_enum(param, enums) else {
            continue;
        };
        let message = enum_collection_message(
            &matched,
            &format!("parameter {} of {}.{}", index, class.name, method.name),
        );
        let location = method_location_with_line(
            &class.name,
            &method.name,
            &method.descriptor,
            artifact_uri,
            line,
        );
        results.push(
            SarifResult::builder()
                .message(message)
                .locations(vec![location])
                .build(),
        );
    }
    results
}

fn check_local_variables(
    class: &Class,
    method: &Method,
    enums: &BTreeSet<String>,
    artifact_uri: Option<&str>,
) -> Vec<SarifResult> {
    let mut results = Vec::new();
    for local in &method.local_variable_types {
        let mut parser = SignatureParser::new(&local.signature);
        let Some(ty) = parser.parse_type_signature() else {
            continue;
        };
        let Some(matched) = match_collection_enum(&ty, enums) else {
            continue;
        };
        let line = method.line_for_offset(local.start_pc);
        let message = enum_collection_message(
            &matched,
            &format!("local {} in {}.{}", local.name, class.name, method.name),
        );
        let location = method_location_with_line(
            &class.name,
            &method.name,
            &method.descriptor,
            artifact_uri,
            line,
        );
        results.push(
            SarifResult::builder()
                .message(message)
                .locations(vec![location])
                .build(),
        );
    }
    results
}

fn match_collection_enum(
    ty: &TypeSignature,
    enums: &BTreeSet<String>,
) -> Option<EnumCollectionMatch> {
    let TypeSignature::Class(class_type) = ty else {
        return None;
    };
    if !TARGET_COLLECTION_TYPES.contains(&class_type.name.as_str()) {
        return None;
    }
    let arg = class_type.type_args.get(0)?;
    let TypeSignature::Class(arg_class) = arg else {
        return None;
    };
    if !enums.contains(&arg_class.name) {
        return None;
    }
    Some(EnumCollectionMatch {
        collection: class_type.name.clone(),
        enum_name: arg_class.name.clone(),
    })
}

fn enum_collection_message(
    matched: &EnumCollectionMatch,
    subject: &str,
) -> serde_sarif::sarif::Message {
    let collection = simple_class_name(&matched.collection);
    let enum_name = simple_class_name(&matched.enum_name);
    result_message(format!(
        "Prefer EnumSet<{enum_name}> over {collection}<{enum_name}> for enum collections ({subject})."
    ))
}

fn simple_class_name(class_name: &str) -> &str {
    class_name
        .rsplit(&['/', '$'][..])
        .next()
        .unwrap_or(class_name)
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_sources(sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("PREFER_ENUMSET"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn prefer_enumset_reports_field_and_method() {
        let sources = vec![SourceFile {
            path: "com/example/ClassOne.java".to_string(),
            contents: r#"
package com.example;
import java.util.HashSet;
import java.util.Set;

enum EnumAlpha { VALUE_ONE, VALUE_TWO }

public class ClassOne {
    private Set<EnumAlpha> fieldOne = new HashSet<>();

    public Set<EnumAlpha> methodOne() {
        return new HashSet<>();
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("field")));
        assert!(messages.iter().any(|msg| msg.contains("return type")));
    }

    #[test]
    fn prefer_enumset_reports_local_variables() {
        let sources = vec![SourceFile {
            path: "com/example/ClassTwo.java".to_string(),
            contents: r#"
package com.example;
import java.util.ArrayList;
import java.util.List;

enum EnumBeta { VALUE_ONE, VALUE_TWO, VALUE_THREE }

public class ClassTwo {
    public void methodOne() {
        List<EnumBeta> localOne = new ArrayList<>();
        localOne.add(EnumBeta.VALUE_THREE);
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("local")));
    }

    #[test]
    fn prefer_enumset_ignores_non_enum_collections() {
        let sources = vec![SourceFile {
            path: "com/example/ClassThree.java".to_string(),
            contents: r#"
package com.example;
import java.util.HashSet;
import java.util.Set;

public class ClassThree {
    private Set<String> fieldOne = new HashSet<>();
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn prefer_enumset_ignores_enum_map_values() {
        let sources = vec![SourceFile {
            path: "com/example/ClassFive.java".to_string(),
            contents: r#"
package com.example;
import java.util.HashMap;
import java.util.Map;

enum EnumDelta { VALUE_ONE, VALUE_TWO }

public class ClassFive {
    private Map<String, EnumDelta> mapOne = new HashMap<>();
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn prefer_enumset_reports_wildcard_enum_set() {
        let sources = vec![SourceFile {
            path: "com/example/ClassSix.java".to_string(),
            contents: r#"
package com.example;
import java.util.HashSet;
import java.util.Set;

enum EnumEpsilon { VALUE_ONE, VALUE_TWO }

public class ClassSix {
    private Set<? extends EnumEpsilon> fieldOne = new HashSet<>();
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("field")));
    }

    #[test]
    fn prefer_enumset_ignores_enumset_usage() {
        let sources = vec![SourceFile {
            path: "com/example/ClassFour.java".to_string(),
            contents: r#"
package com.example;
import java.util.EnumSet;

enum EnumGamma { VALUE_ONE, VALUE_TWO }

public class ClassFour {
    private EnumSet<EnumGamma> fieldOne = EnumSet.noneOf(EnumGamma.class);

    public EnumSet<EnumGamma> methodOne() {
        return EnumSet.noneOf(EnumGamma.class);
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }
}
