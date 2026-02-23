use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde_sarif::sarif::Artifact;
use tracing::warn;

use crate::ir::Class;

/// Resolved classpath index keyed by class name.
pub(crate) struct ClasspathIndex {
    pub(crate) classes: BTreeMap<String, i64>,
}

/// Resolves the classpath index from the given classes and artifacts.
///
/// If `allow_duplicate_classes` is false (the default), duplicate class names
/// across artifacts are treated as an error and the function returns `Err`.
///
/// If `allow_duplicate_classes` is true, duplicates emit a warning and the
/// class from the artifact with the lexicographically smallest URI is used,
/// ensuring deterministic behavior regardless of scan order.
pub(crate) fn resolve_classpath(
    classes: &[Class],
    artifacts: &[Artifact],
    allow_duplicate_classes: bool,
) -> Result<ClasspathIndex> {
    let mut class_map: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for class in classes {
        class_map
            .entry(class.name.clone())
            .or_default()
            .push(class.artifact_index);
    }

    let mut error_duplicates = Vec::new();
    for (name, indices) in &mut class_map {
        if indices.len() <= 1 {
            continue;
        }
        if allow_duplicate_classes {
            // Sort by artifact URI for a deterministic, reproducible selection.
            indices.sort_by(|&a, &b| artifact_uri(artifacts, a).cmp(&artifact_uri(artifacts, b)));
            warn!(
                "duplicate class {} found in multiple artifacts; using {}",
                name,
                artifact_uri(artifacts, indices[0])
            );
        } else {
            let duplicate_artifacts = indices
                .iter()
                .map(|index| format!("{index} ({})", artifact_uri(artifacts, *index)))
                .collect::<Vec<_>>()
                .join(", ");
            error_duplicates.push(format!("{name}: [{duplicate_artifacts}]"));
        }
    }
    if !error_duplicates.is_empty() {
        anyhow::bail!("duplicate classes found: {}", error_duplicates.join(", "));
    }

    let class_names: BTreeSet<String> = class_map.keys().cloned().collect();
    let mut missing = BTreeSet::new();
    for class in classes {
        for reference in &class.referenced_classes {
            if is_platform_class(reference) {
                continue;
            }
            if !class_names.contains(reference) {
                missing.insert(reference.clone());
            }
        }
    }
    let _missing = missing;

    let classes = class_map
        .into_iter()
        .map(|(name, indices)| {
            (
                name,
                indices.into_iter().next().expect("class indices not empty"),
            )
        })
        .collect();

    Ok(ClasspathIndex { classes })
}

/// Returns the URI of the artifact at the given index, or an empty string if unavailable.
fn artifact_uri(artifacts: &[Artifact], index: i64) -> String {
    artifacts
        .get(index as usize)
        .and_then(|a| a.location.as_ref())
        .and_then(|l| l.uri.as_deref())
        .unwrap_or("")
        .to_string()
}

fn is_platform_class(name: &str) -> bool {
    const PREFIXES: [&str; 5] = ["java/", "javax/", "jdk/", "sun/", "com/sun/"];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use serde_sarif::sarif::{Artifact, ArtifactLocation};

    use super::*;

    fn make_artifact(uri: &str) -> Artifact {
        Artifact::builder()
            .location(ArtifactLocation::builder().uri(uri.to_string()).build())
            .build()
    }

    #[test]
    fn resolve_classpath_accepts_java_references() {
        let classes = vec![
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: vec!["java/lang/Object".to_string()],
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 0,
                is_record: false,
            },
            Class {
                name: "com/example/Bar".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 1,
                is_record: false,
            },
        ];

        let result = resolve_classpath(&classes, &[], false);

        assert!(result.is_ok());
    }

    #[test]
    fn resolve_classpath_allows_missing_classes() {
        let classes = vec![Class {
            name: "com/example/Foo".to_string(),
            source_file: None,
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: vec!["com/example/Bar".to_string()],
            fields: Vec::new(),
            methods: Vec::new(),
            annotation_defaults: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }];

        let result = resolve_classpath(&classes, &[], false);

        assert!(result.is_ok());
    }

    #[test]
    fn resolve_classpath_rejects_duplicates() {
        let artifacts = vec![
            make_artifact("file:///first.jar"),
            make_artifact("file:///second.jar"),
        ];
        let classes = vec![
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 0,
                is_record: false,
            },
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 1,
                is_record: false,
            },
        ];

        let result = resolve_classpath(&classes, &artifacts, false);

        assert!(result.is_err());
        let error = result.err().expect("duplicate class error");
        let error_text = format!("{error:#}");
        assert!(error_text.contains("duplicate classes"));
        assert!(error_text.contains("file:///first.jar"));
        assert!(error_text.contains("file:///second.jar"));
    }

    #[test]
    fn resolve_classpath_warns_for_duplicates() {
        let classes = vec![
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 0,
                is_record: false,
            },
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 1,
                is_record: false,
            },
        ];

        let result = resolve_classpath(&classes, &[], true);

        assert!(result.is_ok());
        let index = result.unwrap();
        assert!(index.classes.contains_key("com/example/Foo"));
        assert_eq!(
            index.classes["com/example/Foo"], 0,
            "when artifacts are empty, the first duplicate encountered should win"
        );
    }

    #[test]
    fn resolve_classpath_picks_lex_first_artifact_for_duplicate() {
        // artifact 0 has URI "file:///zzz.jar" (lex-later)
        // artifact 1 has URI "file:///aaa.jar" (lex-first)
        // Expected: the class from artifact 1 is chosen.
        let artifacts = vec![
            make_artifact("file:///zzz.jar"),
            make_artifact("file:///aaa.jar"),
        ];
        let classes = vec![
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 0,
                is_record: false,
            },
            Class {
                name: "com/example/Foo".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 1,
                is_record: false,
            },
        ];

        let result = resolve_classpath(&classes, &artifacts, true);

        assert!(result.is_ok());
        let index = result.unwrap();
        assert_eq!(
            index.classes["com/example/Foo"], 1,
            "should pick artifact 1 (aaa.jar) over artifact 0 (zzz.jar)"
        );
    }
}
