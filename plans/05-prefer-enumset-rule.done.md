# Plan: Rule to Prefer EnumSet Over Other Collections

## Objective
Create a static analysis rule that detects when developers use less efficient collection types (like `List<E>` or `Set<E>`) for enum values, and suggests using `EnumSet<E>` instead.

## Background
`EnumSet` is a specialized `Set` implementation for enum types that:
- Is significantly more efficient than `HashSet` for enum types
- Uses a bit vector representation internally
- Provides better performance for all operations
- Is type-safe and more memory-efficient

Common inefficient patterns:
```java
Set<MyEnum> states = new HashSet<>();           // Should use EnumSet
List<MyEnum> flags = new ArrayList<>();         // Should use EnumSet if used as set
Collection<MyEnum> values = new LinkedList<>(); // Should use EnumSet
```

Better approach:
```java
Set<MyEnum> states = EnumSet.noneOf(MyEnum.class);
Set<MyEnum> flags = EnumSet.of(MyEnum.VALUE1, MyEnum.VALUE2);
```

## Rule Details

### Rule ID
`PREFER_ENUMSET`

### Rule Name
"Prefer EnumSet for enum collections"

### Rule Description
"Using EnumSet instead of general-purpose Set or List implementations for enum types provides better performance and memory efficiency."

### Severity
Warning (not error, as it's an optimization recommendation)

## Detection Strategy

### Pattern 1: Field Declarations
Detect fields declared as `Set<E>`, `List<E>`, or `Collection<E>` where `E` is an enum type:

```java
// Pattern to detect
private Set<MyEnum> states = new HashSet<>();
private List<MyEnum> flags = new ArrayList<>();
private Collection<MyEnum> values = new LinkedList<>();

// Suggested fix
private Set<MyEnum> states = EnumSet.noneOf(MyEnum.class);
```

### Pattern 2: Variable Declarations in Methods
Detect local variables declared with enum collection types:

```java
void methodOne() {
    Set<Status> statuses = new HashSet<>();  // Detect this
    // Suggest: EnumSet<Status> statuses = EnumSet.noneOf(Status.class);
}
```

### Pattern 3: Method Return Types
Detect methods returning `Set<E>`, `List<E>`, or `Collection<E>` where `E` is an enum:

```java
// Pattern to detect
public Set<MyEnum> getStates() {
    return new HashSet<>();
}

// Suggested improvement
public Set<MyEnum> getStates() {
    return EnumSet.noneOf(MyEnum.class);
}
```

### Pattern 4: Constructor Calls
Detect instantiation of `HashSet`, `TreeSet`, `LinkedHashSet`, `ArrayList`, etc. with enum type parameters:

```java
new HashSet<MyEnum>()
new ArrayList<MyEnum>()
new TreeSet<MyEnum>()
```

## Implementation Approach

### Bytecode Analysis
1. **Identify enum types**: Track which classes extend `java.lang.Enum`
2. **Analyze field declarations**: Check signatures for collection types with enum parameters
3. **Analyze method signatures**: Check return types and parameter types
4. **Track object instantiation**: Look for `NEW` opcodes followed by `INVOKESPECIAL` for collection constructors
5. **Examine type parameters**: Parse generic signatures to identify enum type arguments

### Type Information Required
- Map of class names to their superclass (to identify enums)
- Generic signature parsing for parameterized types
- Collection type recognition (`java.util.Set`, `java.util.List`, `java.util.Collection`)

### Bytecode Instructions to Monitor
- `NEW` + `INVOKESPECIAL` for `java/util/HashSet`, `ArrayList`, etc.
- Field signatures in `FieldInfo` structures
- Method signatures in `MethodInfo` structures
- Local variable type annotations

## Implementation Steps

### 1. Create Rule Structure
```rust
// src/rules/prefer_enumset.rs
pub(crate) struct PreferEnumSetRule;

impl Rule for PreferEnumSetRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "PREFER_ENUMSET",
            name: "Prefer EnumSet for enum collections",
            description: "Using EnumSet for enum types provides better performance than HashSet or ArrayList",
        }
    }
    
    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        // Implementation
    }
}
```

### 2. Identify Enum Types
Build a set of enum class names during initial scan:
```rust
fn identify_enums(context: &AnalysisContext) -> BTreeSet<String> {
    context.classes
        .iter()
        .filter(|c| c.super_class.as_deref() == Some("java/lang/Enum"))
        .map(|c| c.name.clone())
        .collect()
}
```

### 3. Check Field Declarations
Parse field signatures to detect enum collections:
```rust
fn check_field_declarations(class: &Class, enums: &BTreeSet<String>) -> Vec<SarifResult> {
    // Parse field signatures
    // Detect: Ljava/util/Set<LMyEnum;>
    // Report if type parameter is an enum
}
```

### 4. Check Method Signatures
Analyze return types and parameters:
```rust
fn check_method_signatures(method: &Method, enums: &BTreeSet<String>) -> Vec<SarifResult> {
    // Parse method signature
    // Check return type and parameters
}
```

### 5. Check Object Instantiation
Detect `new HashSet<EnumType>()` patterns:
```rust
fn check_instantiation(method: &Method, enums: &BTreeSet<String>) -> Vec<SarifResult> {
    // Look for NEW opcode for HashSet, ArrayList, etc.
    // Check if type parameter is an enum
}
```

### 6. Add to Rules Module
Update `src/rules/mod.rs`:
```rust
mod prefer_enumset;
use prefer_enumset::PreferEnumSetRule;

pub(crate) fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // ... existing rules ...
        Box::new(PreferEnumSetRule),
    ]
}
```

## Test Cases

### Test 1: Field Declaration
```java
enum Status { ACTIVE, INACTIVE }

public class ClassA {
    private Set<Status> statusSet = new HashSet<>();  // Should report
    private EnumSet<Status> goodSet = EnumSet.noneOf(Status.class);  // Should NOT report
}
```

### Test 2: Method Return Type
```java
enum Color { RED, GREEN, BLUE }

public class ClassB {
    public Set<Color> getColors() {  // Should report
        return new HashSet<>();
    }
    
    public EnumSet<Color> getColorsCorrectly() {  // Should NOT report
        return EnumSet.noneOf(Color.class);
    }
}
```

### Test 3: Local Variable
```java
enum Priority { LOW, MEDIUM, HIGH }

public class ClassC {
    void methodOne() {
        List<Priority> priorities = new ArrayList<>();  // Should report
        priorities.add(Priority.HIGH);
    }
}
```

### Test 4: Non-Enum Types (Should NOT Report)
```java
public class ClassD {
    private Set<String> strings = new HashSet<>();  // Should NOT report (not enum)
    private List<Integer> numbers = new ArrayList<>();  // Should NOT report (not enum)
}
```

### Test 5: Mixed Usage
```java
enum Role { ADMIN, USER, GUEST }

public class ClassE {
    private Set<Role> roles = new HashSet<>();  // Should report
    private Set<String> names = new HashSet<>();  // Should NOT report
    private EnumSet<Role> goodRoles = EnumSet.noneOf(Role.class);  // Should NOT report
}
```

## SARIF Output Example
```json
{
  "ruleId": "PREFER_ENUMSET",
  "level": "warning",
  "message": {
    "text": "Consider using EnumSet<Status> instead of HashSet<Status> for better performance with enum types"
  },
  "locations": [{
    "physicalLocation": {
      "artifactLocation": { "uri": "file:///app.jar" },
      "region": { "startLine": 5 }
    }
  }]
}
```

## Edge Cases to Consider
1. Enum used as value type in `Map<String, MyEnum>` - should NOT report
2. `Set<? extends MyEnum>` - wildcard types
3. `Set<Object>` containing enums at runtime - cannot detect statically
4. Third-party library methods requiring `Set<E>` interface - may need suppressions
5. Serialization concerns - `EnumSet` is serializable but has special handling

## Future Enhancements
- Suggest specific `EnumSet` factory method based on usage:
  - `EnumSet.noneOf()` for empty initialization
  - `EnumSet.allOf()` when all values are used
  - `EnumSet.of()` for specific values
- Auto-fix capability (if inspequte supports it)
- Performance impact metrics in the report

## Success Criteria
- Correctly identifies `Set<E>`, `List<E>`, `Collection<E>` where E is enum
- Does NOT report for non-enum types
- Does NOT report when `EnumSet` is already used
- Provides clear, actionable message
- Includes proper location information (class, method, line)
- Test coverage for all patterns

## Dependencies
- Generic signature parsing capability
- Enum type identification
- Existing rule infrastructure

## Estimated Complexity
**Medium** - Requires generic signature parsing and type analysis, but pattern is straightforward.
