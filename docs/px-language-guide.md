# The Praxis Intent Language (.px)

`.px` is a declarative language for expressing logic, constraints, procedures, and configuration. It compiles to PluresDB records that drive reactive behavior.

## Philosophy

> Express **what** should happen, not **how** to make it happen.

`.px` sits between natural language and code. It's readable by humans, executable by machines, and inspectable by AI agents. The compiler produces structured data that the PluresDB procedure engine executes reactively.

## File Structure

A `.px` file contains one or more top-level declarations:

```px
# Comments start with #

config app:
  setting: "value"

constraint my_rule:
  when: context.ready == true
  require: context.tests_passed == true
  severity: error
  message: "Tests must pass"

procedure my_workflow:
  trigger: manual
  given: "Description of what this does"
  db_get key: "some:key" -> $data
  db_put key: "result" value: $data
```

## Declarations

### config — Static Configuration

```px
config block_name:
  key1: "string value"
  key2: 42
  key3: true
  key4: 3.14
```

Config blocks group related settings. Values can be strings, numbers, or booleans.

### constraint — Validation Rules

```px
constraint name:
  when: <expression>
  require: <expression>
  severity: error | warning | info
  message: "Human-readable explanation"
```

Constraints are continuously evaluated. When the `when` condition is true and the `require` condition is false, a violation is raised.

**Optional fields:**
- `scope: <ident>` — limit where this constraint applies
- `phase: pre-push, pre-release` — when to evaluate
- `weight: 0.8` — relative importance (0.0–1.0)

### procedure — Executable Workflows

```px
procedure name:
  trigger: manual | on_write | cron
  given: "What this procedure does"
  <steps>
```

Procedures are sequences of steps that execute in order. They can read/write state, call tools, branch conditionally, and loop.

### rule — Event-Driven Logic

```px
rule name:
  priority: 10
  when:
    - event.type == "deployment"
    - event.target == "production"
  then:
    - action: notify param: "deploying"
```

Rules fire when their conditions match incoming events.

### fact — Named State Schema

```px
fact name:
  field1: string
  field2: int
  field3: bool
```

Facts define the shape of state entries stored in PluresDB.

### entity — Domain Objects

```px
entity User:
  prefix: "user:"
  fields:
    name: String
    role: enum(admin, editor, viewer)
    active: bool
```

Entities define structured objects with typed fields.

### function — Pure Computations

```px
function distance(x1: float, y1: float, x2: float, y2: float) -> float:
  mode: deterministic
  """Euclidean distance between two points"""
```

Functions declare pure computations. `mode: deterministic` means same inputs always produce same outputs.

### trigger — Reactive Activation

```px
trigger name:
  on: after_store | before_search | timer | on_event("custom")
  schedule: "0 */2 * * *"
  run: procedure_name
```

Triggers activate procedures in response to events or schedules.

### scenario — Test Cases

```px
scenario name:
  given: "Setup context"
  setup:
    db_put key: "test:val" value: 42
  run: my_procedure
  expect:
    - db_has key: "test:val" value: 42
```

Scenarios are executable test cases that verify procedure behavior.

### contract — Behavioral Specifications

```px
contract name:
  given: "Context"
  when: "Trigger condition"
  then: "Expected outcome"
  threshold: 0.95
  examples:
    - input: "hello"
      expect: "greeting"
```

Contracts define expected behavior with examples for testing.

## Procedure Steps

Steps are the building blocks of procedures. They execute sequentially unless branching or looping.

### Action Calls

Call a named action (tool) with parameters:

```px
db_get key: "my:key" -> $result
db_put key: "output" value: $result
run_command cmd: "ls -la" -> $output
web_fetch url: "https://example.com" -> $page
```

The `-> $variable` suffix captures the result.

### Variable Assignment

```px
$count = 0
$name = "radix"
$config = $settings.model
```

### Conditional (if/else)

```px
if $count > 10:
  db_put key: "status" value: "overflow"
else:
  db_put key: "status" value: "ok"
end
```

### Loops (for)

```px
for $item in $list:
  db_put key: $item.id value: $item
end
```

### Pattern Matching (match)

The `match:` step selects an arm based on expression evaluation:

```px
match:
  $role == "admin" -> grant_access
  $role == "viewer" -> read_only
end
```

### Emit (Events)

Fire an event into the reactive pipeline:

```px
emit event: "task.completed" data: {id: $task_id}
```

### Loop (over/times)

```px
loop over $items as item:
  process_item input: $item -> $result
end

loop times 5 as i:
  retry_operation attempt: $i
end
```

### Try/Catch

```px
try retry 3 delay 1000 ms backoff exponential:
  risky_operation param: $value -> $result
catch:
  db_put key: "error" value: "operation failed"
end
```

### Parallel Branches

```px
parallel -> $results:
  branch fetch_a:
    web_fetch url: "https://api-a.com" -> $a
  end
  branch fetch_b:
    web_fetch url: "https://api-b.com" -> $b
  end
end
```

### Return / Abort

```px
return $result
abort "Something went wrong"
```

## Expressions

Expressions are used in conditions, assignments, and parameters.

### Operators

| Operator | Meaning |
|----------|---------|
| `==`, `!=` | Equality |
| `>`, `<`, `>=`, `<=` | Comparison |
| `+`, `-`, `*`, `/` | Arithmetic |
| `^` | Power |
| `&&`, `and` | Logical AND |
| `\|\|`, `or` | Logical OR |
| `NOT` | Logical negation |

### Values

- Strings: `"hello"` or `'hello'`
- Numbers: `42`, `3.14`
- Booleans: `true`, `false`
- Lists: `[1, 2, 3]`
- Maps: `{key: "value", count: 5}`
- Variables: `$name`, `$config.nested.field`

### Built-in Functions

Available via NativeFunctionRegistry:

| Function | Description |
|----------|-------------|
| `sqrt(x)` | Square root |
| `sin(x)`, `cos(x)` | Trigonometry |
| `abs(x)` | Absolute value |
| `min(a, b)`, `max(a, b)` | Min/max |
| `exp(x)` | Exponential |
| `pi()` | π constant |
| `random()` | Random 0.0–1.0 |

## Variable References

Variables are prefixed with `$` and can be:
- Bound by action results: `db_get key: "x" -> $val`
- Assigned directly: `$count = 0`
- Dotted for nested access: `$config.model.name`
- Used in expressions: `$count + 1`

## Best Practices

1. **Start with .px** — Express intent before implementation
2. **One concern per file** — Keep files focused (config, constraints, procedures)
3. **Name things clearly** — Procedure names should read like commands: `route_code`, `validate_architecture`
4. **Use constraints for invariants** — Things that must ALWAYS be true
5. **Use procedures for workflows** — Things that happen in response to events
6. **Test with scenarios** — Every procedure should have a scenario that verifies it
7. **Keep steps atomic** — Each step should do one thing clearly

## Tooling

```bash
# Parse and check syntax
pares-radix px check file.px

# Compile to PluresDB records
pares-radix px compile file.px

# Lint for common issues
pares-radix px lint file.px

# Run a procedure manually
pares-radix px run --procedure my_workflow
```

## Further Reading

- [Configuration Guide](./configuration.md) — using .px for radix config
- [Procedure Cookbook](./procedure-cookbook.md) — real-world procedure examples
- [Constraint Patterns](./constraint-patterns.md) — effective constraint design
