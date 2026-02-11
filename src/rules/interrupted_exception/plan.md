# Plan: Rule for Proper InterruptedException Handling

## Objective
Create a static analysis rule to detect improper handling of `InterruptedException` and ensure threads follow multi-threading best practices by restoring the interrupt status.

## Background
When a thread is interrupted while blocked (e.g., in `Thread.sleep()`, `Object.wait()`, or blocking I/O), an `InterruptedException` is thrown. Proper handling requires restoring the thread's interrupt status so that higher-level code can respond appropriately.

### Improper Handling (Anti-pattern)
```java
public void run() {
    try {
        Thread.sleep(1000);
    } catch (InterruptedException e) {
        // BUG: Interrupt status lost!
        e.printStackTrace();
    }
}
```

### Proper Handling
```java
public void run() {
    try {
        Thread.sleep(1000);
    } catch (InterruptedException e) {
        Thread.currentThread().interrupt();  // Restore interrupt status
    }
}
```

### Alternative Proper Handling
```java
public void run() throws InterruptedException {
    Thread.sleep(1000);  // Let caller handle interruption
}
```

## Rule Details

### Rule ID
`INTERRUPTED_EXCEPTION_NOT_RESTORED`

### Rule Name
"InterruptedException not properly handled"

### Rule Description
"When catching InterruptedException, either restore the interrupt status by calling Thread.currentThread().interrupt() or propagate the exception to the caller. Swallowing the exception without restoring interrupt status can cause thread shutdown delays and unresponsive applications."

### Severity
Warning (can lead to production issues but not security vulnerabilities)

## Detection Strategy

### Pattern to Detect
Catch blocks for `InterruptedException` that:
1. Do NOT call `Thread.currentThread().interrupt()`
2. Do NOT re-throw the exception (or wrap and throw)
3. Simply log, ignore, or perform other actions

### Must Check
- All catch handlers for `java.lang.InterruptedException`
- All catch handlers for `java.lang.Exception` (may catch IE)
- Multi-catch statements including `InterruptedException`

## Implementation Approach

### Bytecode Analysis Steps

1. **Identify exception handlers**: Parse exception table in method bytecode
2. **Find InterruptedException handlers**: Look for handlers catching `java/lang/InterruptedException`
3. **Analyze handler code**: Check the bytecode within the handler block
4. **Verify proper handling**: Ensure one of these patterns exists:
   - Call to `Thread.currentThread().interrupt()`
   - Re-throw (via `ATHROW` instruction)
   - Throw wrapped exception

### Bytecode Instructions to Monitor

#### Proper Handling Pattern 1: Restore Interrupt
```
INVOKESTATIC java/lang/Thread.currentThread()Ljava/lang/Thread;
INVOKEVIRTUAL java/lang/Thread.interrupt()V
```

#### Proper Handling Pattern 2: Re-throw
```
ALOAD <exception variable>
ATHROW
```

#### Proper Handling Pattern 3: Wrap and Throw
```
NEW java/lang/RuntimeException
DUP
ALOAD <exception variable>
INVOKESPECIAL java/lang/RuntimeException.<init>(Ljava/lang/Throwable;)V
ATHROW
```

### Exception Table Structure
```rust
struct ExceptionHandler {
    start_pc: u16,
    end_pc: u16,
    handler_pc: u16,
    catch_type: Option<String>,  // "java/lang/InterruptedException"
}
```

## Implementation Steps

### 1. Create Rule Structure
```rust
// src/rules/interrupted_exception.rs
pub(crate) struct InterruptedExceptionRule;

impl Rule for InterruptedExceptionRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "INTERRUPTED_EXCEPTION_NOT_RESTORED",
            name: "InterruptedException not properly handled",
            description: "Restore interrupt status when catching InterruptedException",
        }
    }
    
    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        // Implementation
    }
}
```

### 2. Parse Exception Tables
Extract exception handlers from method bytecode:
```rust
fn get_exception_handlers(method: &Method) -> Vec<ExceptionHandler> {
    // Parse exception_table attribute from Code attribute
}
```

### 3. Filter InterruptedException Handlers
```rust
fn find_interrupted_exception_handlers(
    handlers: &[ExceptionHandler],
    constant_pool: &ConstantPool
) -> Vec<ExceptionHandler> {
    handlers.iter()
        .filter(|h| is_interrupted_exception(h.catch_type, constant_pool))
        .collect()
}

fn is_interrupted_exception(catch_type: &Option<String>, cp: &ConstantPool) -> bool {
    if let Some(type_name) = catch_type {
        type_name == "java/lang/InterruptedException" ||
        type_name == "java/lang/Exception" ||  // May catch IE
        type_name == "java/lang/Throwable"     // May catch IE
    } else {
        false
    }
}
```

### 4. Analyze Handler Code
Check bytecode in handler for proper interrupt restoration:
```rust
fn check_handler_code(
    method: &Method,
    handler: &ExceptionHandler
) -> bool {
    let handler_code = extract_handler_bytecode(method, handler);
    
    has_interrupt_call(&handler_code) || 
    has_rethrow(&handler_code) ||
    has_wrapped_throw(&handler_code)
}

fn has_interrupt_call(bytecode: &[Instruction]) -> bool {
    // Look for pattern:
    // INVOKESTATIC Thread.currentThread()
    // INVOKEVIRTUAL Thread.interrupt()
}

fn has_rethrow(bytecode: &[Instruction]) -> bool {
    // Look for ATHROW instruction
}
```

### 5. Generate SARIF Results
```rust
fn create_violation(
    class: &Class,
    method: &Method,
    handler: &ExceptionHandler
) -> SarifResult {
    // Create SARIF result with location info
}
```

### 6. Add to Rules Module
Update `src/rules/mod.rs`:
```rust
mod interrupted_exception;
use interrupted_exception::InterruptedExceptionRule;

pub(crate) fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // ... existing rules ...
        Box::new(InterruptedExceptionRule),
    ]
}
```

## Test Cases

### Test 1: Improper Handling (Should Report)
```java
public class ClassA {
    public void methodOne() {
        try {
            Thread.sleep(1000);
        } catch (InterruptedException e) {
            // BUG: No interrupt restoration
            System.out.println("Interrupted");
        }
    }
}
```

### Test 2: Proper Handling - Restore Interrupt (Should NOT Report)
```java
public class ClassB {
    public void methodTwo() {
        try {
            Thread.sleep(1000);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();  // Correct!
        }
    }
}
```

### Test 3: Proper Handling - Re-throw (Should NOT Report)
```java
public class ClassC {
    public void methodThree() throws InterruptedException {
        Thread.sleep(1000);  // Propagates exception
    }
}
```

### Test 4: Proper Handling - Wrap and Throw (Should NOT Report)
```java
public class ClassD {
    public void methodFour() {
        try {
            Thread.sleep(1000);
        } catch (InterruptedException e) {
            throw new RuntimeException("Interrupted", e);  // Correct!
        }
    }
}
```

### Test 5: Multi-catch (Should Report)
```java
public class ClassE {
    public void methodFive() {
        try {
            Thread.sleep(1000);
        } catch (InterruptedException | IllegalArgumentException e) {
            // BUG: No interrupt restoration
            e.printStackTrace();
        }
    }
}
```

### Test 6: Catch Exception (Should Report)
```java
public class ClassF {
    public void methodSix() {
        try {
            Thread.sleep(1000);
        } catch (Exception e) {
            // BUG: Might catch InterruptedException
            System.out.println(e);
        }
    }
}
```

### Test 7: Proper with Logging (Should NOT Report)
```java
public class ClassG {
    public void methodSeven() {
        try {
            Thread.sleep(1000);
        } catch (InterruptedException e) {
            logger.warn("Interrupted", e);
            Thread.currentThread().interrupt();  // Correct!
        }
    }
}
```

### Test 8: Finally Block Restoration (Should NOT Report)
```java
public class ClassH {
    public void methodEight() {
        boolean interrupted = false;
        try {
            Thread.sleep(1000);
        } catch (InterruptedException e) {
            interrupted = true;
        } finally {
            if (interrupted) {
                Thread.currentThread().interrupt();  // Correct!
            }
        }
    }
}
```

## SARIF Output Example
```json
{
  "ruleId": "INTERRUPTED_EXCEPTION_NOT_RESTORED",
  "level": "warning",
  "message": {
    "text": "InterruptedException is caught but interrupt status is not restored. Call Thread.currentThread().interrupt() in the catch block or propagate the exception."
  },
  "locations": [{
    "physicalLocation": {
      "artifactLocation": { "uri": "file:///app.jar" },
      "region": { 
        "startLine": 5,
        "snippet": { "text": "catch (InterruptedException e)" }
      }
    }
  }]
}
```

## Edge Cases to Consider

1. **Finally blocks**: Check if interrupt is called in finally block
2. **Multiple catch blocks**: Handle multi-catch and sequential catches
3. **Catch Exception/Throwable**: May inadvertently catch InterruptedException
4. **Nested try-catch**: Handler might be in outer block
5. **Conditional interrupt**: `if (someCondition) Thread.currentThread().interrupt()`
6. **Delegated handling**: Calling a method that restores interrupt

## False Positive Mitigation

### Cases to NOT Report
1. Exception is re-thrown or wrapped and thrown
2. `Thread.currentThread().interrupt()` is called anywhere in handler
3. Interrupt is restored in finally block
4. Method signature declares `throws InterruptedException`

### Cases That May Need Suppression
1. Framework code that has its own interrupt handling
2. Test code where interrupt behavior is intentionally tested
3. Cleanup code in shutdown hooks

## Additional Checks (Future Enhancement)

1. **Empty catch block**: Report even higher severity for completely empty handlers
2. **Only logging**: Specifically flag cases where exception is only logged
3. **Interrupt in finally**: Detect pattern where boolean flag tracks interruption
4. **Control flow analysis**: Track if all paths restore interrupt

## References
- Java Concurrency in Practice (Brian Goetz) - Chapter 7
- Effective Java (Joshua Bloch) - Item 73
- Java Language Specification - Thread interruption semantics

## Success Criteria
- Detects catch blocks for `InterruptedException` without proper handling
- Does NOT report when `Thread.currentThread().interrupt()` is called
- Does NOT report when exception is re-thrown or wrapped
- Provides clear message explaining the issue and fix
- Test coverage for all handling patterns
- Low false positive rate

## Dependencies
- Exception table parsing from bytecode
- Control flow analysis within catch blocks
- Existing rule infrastructure

## Estimated Complexity
**Medium** - Requires exception table parsing and bytecode pattern matching within catch blocks.
