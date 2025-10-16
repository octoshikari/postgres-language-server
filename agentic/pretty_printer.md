# Pretty Printer Implementation Plan

## Overview

This document outlines the plan to complete the implementation of the Postgres SQL pretty printer in `crates/pgt_pretty_print/`. The pretty printer takes parsed SQL AST nodes (from `pgt_query`) and emits formatted SQL code that respects line length constraints while maintaining semantic correctness.

## ⚠️ SCOPE: Implementation Task

**THIS TASK IS ONLY ABOUT IMPLEMENTING `emit_*` FUNCTIONS IN `src/nodes/`**

- ✅ **DO**: Implement `emit_*` functions for each AST node type
- ✅ **DO**: Add new files to `src/nodes/` for each node type
- ✅ **DO**: Update `src/nodes/mod.rs` to dispatch new node types
- ✅ **DO**: Use existing helpers in `node_list.rs` and `string.rs`
- ✅ **DO**: Keep this document updated with progress and learnings
- ❌ **DON'T**: Modify the renderer (`src/renderer.rs`)
- ❌ **DON'T**: Modify the emitter (`src/emitter.rs`)
- ❌ **DON'T**: Change the test infrastructure (`tests/tests.rs`)
- ❌ **DON'T**: Modify code generation (`src/codegen/`)

The renderer, emitter, and test infrastructure are already complete and working correctly. Your job is to implement the missing `emit_*` functions so that all AST nodes can be formatted.

## 📝 CRITICAL: Keep This Document Updated

**As you implement nodes, update the following sections:**

1. **Completed Nodes section** - Mark nodes as `[x]` when done, add notes about partial implementations
2. **Implementation Learnings section** (below) - Document patterns, gotchas, and decisions
3. **Progress tracking** - Update the count (e.g., "14/270 → 20/270")

**This allows stopping and restarting work at any time!**

## Architecture

### Core Components

1. **EventEmitter** (`src/emitter.rs`)
   - Emits layout events (tokens, spaces, lines, groups, indents)
   - Events are later processed by the renderer to produce formatted output

2. **Renderer** (`src/renderer.rs`)
   - Converts layout events into actual formatted text
   - Handles line breaking decisions based on `max_line_length`
   - Implements group-based layout algorithm

3. **Node Emission** (`src/nodes/`)
   - One file per AST node type (e.g., `select_stmt.rs`, `a_expr.rs`)
   - Each file exports an `emit_*` function that takes `&mut EventEmitter` and the node

4. **Code Generation** (`src/codegen/`)
   - `TokenKind`: Generated enum for all SQL tokens (keywords, operators, punctuation)
   - `GroupKind`: Generated enum for logical groupings of nodes

## Implementation Pattern

### Standard Node Emission Pattern

Each `emit_*` function follows this pattern:

```rust
pub(super) fn emit_<node_name>(e: &mut EventEmitter, n: &<NodeType>) {
    // 1. Start a group for this node
    e.group_start(GroupKind::<NodeName>);

    // 2. Emit keywords
    e.token(TokenKind::KEYWORD_KW);

    // 3. Emit child nodes with spacing/line breaks
    if let Some(ref child) = n.child {
        e.space(); // or e.line(LineType::SoftOrSpace)
        super::emit_node(child, e);
    }

    // 4. Emit lists with separators
    emit_comma_separated_list(e, &n.items, super::emit_node);

    // 5. End the group
    e.group_end();
}
```

### Pattern Variations and Examples

#### 1. Simple Node with Fields (RangeVar)

When a node has simple string fields and no optional complex children:

```rust
// src/nodes/range_var.rs
pub(super) fn emit_range_var(e: &mut EventEmitter, n: &RangeVar) {
    e.group_start(GroupKind::RangeVar);

    // Emit qualified name: schema.table
    if !n.schemaname.is_empty() {
        e.token(TokenKind::IDENT(n.schemaname.clone()));
        e.token(TokenKind::DOT);
    }

    e.token(TokenKind::IDENT(n.relname.clone()));

    e.group_end();
}
```

**Key points**:
- No spaces around DOT token
- Check if optional fields are empty before emitting
- Use `TokenKind::IDENT(String)` for identifiers

#### 2. Node with List Helper (ColumnRef)

When a node primarily wraps a list:

```rust
// src/nodes/column_ref.rs
pub(super) fn emit_column_ref(e: &mut EventEmitter, n: &ColumnRef) {
    e.group_start(GroupKind::ColumnRef);
    emit_dot_separated_list(e, &n.fields);
    e.group_end();
}
```

**Key points**:
- Delegate to helper functions in `node_list.rs`
- Available helpers:
  - `emit_comma_separated_list(e, nodes, render_fn)`
  - `emit_dot_separated_list(e, nodes)`
  - `emit_keyword_separated_list(e, nodes, keyword)`

#### 3. Context-Specific Emission (ResTarget)

When a node needs different formatting based on context (SELECT vs UPDATE):

```rust
// src/nodes/res_target.rs

// For SELECT target list: "expr AS alias"
pub(super) fn emit_res_target(e: &mut EventEmitter, n: &ResTarget) {
    e.group_start(GroupKind::ResTarget);

    if let Some(ref val) = n.val {
        emit_node(val, e);
    } else {
        return;
    }

    emit_column_name_with_indirection(e, n);

    if !n.name.is_empty() {
        e.space();
        e.token(TokenKind::AS_KW);
        e.space();
        emit_identifier(e, &n.name);
    }

    e.group_end();
}

// For UPDATE SET clause: "column = expr"
pub(super) fn emit_set_clause(e: &mut EventEmitter, n: &ResTarget) {
    e.group_start(GroupKind::ResTarget);

    if n.name.is_empty() {
        return;
    }

    emit_column_name_with_indirection(e, n);

    if let Some(ref val) = n.val {
        e.space();
        e.token(TokenKind::IDENT("=".to_string()));
        e.space();
        emit_node(val, e);
    }

    e.group_end();
}

// Shared helper for column name with array/field access
pub(super) fn emit_column_name_with_indirection(e: &mut EventEmitter, n: &ResTarget) {
    if n.name.is_empty() {
        return;
    }

    e.token(TokenKind::IDENT(n.name.clone()));

    for i in &n.indirection {
        match &i.node {
            // Field selection: column.field
            Some(pgt_query::NodeEnum::String(n)) => super::emit_string_identifier(e, n),
            // Other indirection types (array access, etc.)
            Some(n) => super::emit_node_enum(n, e),
            None => {}
        }
    }
}
```

**Key points**:
- Export multiple `pub(super)` functions for different contexts
- Share common logic in helper functions
- Handle indirection (array access, field selection) carefully

#### 4. Using `assert_node_variant!` Macro (UpdateStmt)

When you need to extract a specific node variant from a generic `Node`:

```rust
// src/nodes/update_stmt.rs
use crate::nodes::res_target::emit_set_clause;

pub(super) fn emit_update_stmt(e: &mut EventEmitter, n: &UpdateStmt) {
    e.group_start(GroupKind::UpdateStmt);

    e.token(TokenKind::UPDATE_KW);
    e.space();

    if let Some(ref range_var) = n.relation {
        super::emit_range_var(e, range_var)
    }

    if !n.target_list.is_empty() {
        e.space();
        e.token(TokenKind::SET_KW);
        e.space();

        // Use assert_node_variant! to extract ResTarget from generic Node
        emit_comma_separated_list(e, &n.target_list, |n, e| {
            emit_set_clause(e, assert_node_variant!(ResTarget, n))
        });
    }

    if let Some(ref where_clause) = n.where_clause {
        e.space();
        e.token(TokenKind::WHERE_KW);
        e.space();
        emit_node(where_clause, e);
    }

    e.token(TokenKind::SEMICOLON);

    e.group_end();
}
```

**Key points**:
- `assert_node_variant!(NodeType, expr)` extracts a specific node type
- Use this when you know the list contains a specific node type
- Panics if the variant doesn't match (design-time check)
- Useful in closures passed to list helpers

### Important Macros and Helpers

#### `assert_node_variant!` Macro

Defined in `src/nodes/mod.rs`:

```rust
macro_rules! assert_node_variant {
    ($variant:ident, $expr:expr) => {
        match $expr.node.as_ref() {
            Some(pgt_query::NodeEnum::$variant(inner)) => inner,
            other => panic!("Expected {}, got {:?}", stringify!($variant), other),
        }
    };
}
```

**Usage**:
```rust
// When you have a Node and need a specific type
let res_target = assert_node_variant!(ResTarget, node);
emit_res_target(e, res_target);

// In closures for list helpers
emit_comma_separated_list(e, &n.target_list, |node, e| {
    let res_target = assert_node_variant!(ResTarget, node);
    emit_res_target(e, res_target);
});
```

**When to use**:
- When iterating over a `Vec<Node>` that you know contains specific types
- The macro panics at runtime if the type doesn't match (indicates a bug)
- This is better than unwrapping because it provides a clear error message

#### Node Dispatch Pattern

The main dispatch in `src/nodes/mod.rs`:

```rust
pub fn emit_node(node: &Node, e: &mut EventEmitter) {
    if let Some(ref inner) = node.node {
        emit_node_enum(inner, e)
    }
}

pub fn emit_node_enum(node: &NodeEnum, e: &mut EventEmitter) {
    match &node {
        NodeEnum::SelectStmt(n) => emit_select_stmt(e, n),
        NodeEnum::UpdateStmt(n) => emit_update_stmt(e, n),
        // ... more cases
        _ => todo!("emit_node_enum: unhandled node type {:?}", node),
    }
}
```

**To add a new node**:
1. Create `src/nodes/<node_name>.rs`
2. Add `mod <node_name>;` to `src/nodes/mod.rs`
3. Add `use <node_name>::emit_<node_name>;` to imports
4. Add case to `emit_node_enum` match

### Layout Event Types

- **Token**: An actual SQL keyword/operator/identifier (e.g., `SELECT`, `+`, `,`)
- **Space**: A single space character
- **Line**: A line break with different behaviors:
  - `Hard`: Always breaks (e.g., after semicolon)
  - `Soft`: Breaks if group doesn't fit
  - `SoftOrSpace`: Becomes a space if group fits, line break otherwise
- **GroupStart/GroupEnd**: Logical grouping for layout decisions
- **IndentStart/IndentEnd**: Increase/decrease indentation level

### Inspirations from Go Parser

The Go parser in `parser/ast/*.go` provides reference implementations via `SqlString()` methods:

1. **Statement Files**:
   - `statements.go`: SELECT, INSERT, UPDATE, DELETE, CREATE, DROP
   - `ddl_statements.go`: CREATE TABLE, ALTER TABLE, etc.
   - `administrative_statements.go`: GRANT, REVOKE, etc.
   - `utility_statements.go`: COPY, VACUUM, etc.

2. **Expression Files**:
   - `expressions.go`: A_Expr, BoolExpr, ColumnRef, FuncCall, etc.
   - `type_coercion_nodes.go`: TypeCast, CollateClause, etc.

3. **Key Methods to Reference**:
   - `SqlString()`: Returns the SQL string representation
   - `FormatFullyQualifiedName()`: Handles schema.table.column formatting
   - `QuoteIdentifier()`: Adds quotes when needed
   - `FormatCommaList()`: Comma-separated lists

### Inspiration from pgFormatter

Use `pgFormatter` to get ideas about line breaking and formatting decisions:

```bash
# Format a test file to see how pgFormatter would handle it
pg_format tests/data/single/your_test_80.sql

# Format with specific line width
pg_format -w 60 tests/data/single/your_test_60.sql

# Format and output to file for comparison
pg_format tests/data/single/complex_query_80.sql > /tmp/formatted.sql
```

**When to use pgFormatter for inspiration**:
- **Line breaking decisions**: Where should clauses break?
- **Indentation levels**: How much to indent nested structures?
- **Spacing conventions**: Spaces around operators, keywords, etc.
- **Complex statements**: JOINs, CTEs, window functions, etc.

**Important notes**:
- pgFormatter output is for **inspiration only** - don't copy exactly
- Our pretty printer uses a **group-based algorithm** (different from pgFormatter)
- Focus on using **groups and line types** (Soft, SoftOrSpace, Hard) rather than trying to replicate exact output
- pgFormatter might make different choices - that's OK! Use it as a reference, not a spec

**Example workflow**:
```bash
# 1. Create your test case
echo "SELECT a, b, c FROM table1 JOIN table2 ON table1.id = table2.id WHERE x > 10" > tests/data/single/join_example_80.sql

# 2. See how pgFormatter would format it
pg_format -w 80 tests/data/single/join_example_80.sql

# 3. Use that as inspiration for your emit_* implementation
# 4. Run your test to see your output
cargo test -p pgt_pretty_print test_single__join_example_80 -- --show-output

# 5. Iterate on your implementation
```

### Mapping Go to Rust

| Go Pattern | Rust Pattern |
|------------|--------------|
| `parts = append(parts, "SELECT")` | `e.token(TokenKind::SELECT_KW)` |
| `strings.Join(parts, " ")` | Sequential `e.space()` calls |
| `strings.Join(items, ", ")` | `emit_comma_separated_list(...)` |
| `fmt.Sprintf("(%s)", expr)` | `e.token(LPAREN)`, emit, `e.token(RPAREN)` |
| String concatenation | Layout events (token + space/line) |
| `if condition { append(...) }` | `if condition { e.token(...) }` |

## Test Suite

### Test Structure

Tests are located in `tests/`:

1. **Single Statement Tests** (`tests/data/single/*.sql`)
   - Format: `<description>_<line_length>.sql`
   - Example: `simple_select_80.sql` → max line length of 80
   - Each test contains a single SQL statement

2. **Multi Statement Tests** (`tests/data/multi/*.sql`)
   - Format: `<description>_<line_length>.sql`
   - Contains multiple SQL statements separated by semicolons

### Running Tests

```bash
# Run all pretty print tests
cargo test -p pgt_pretty_print

# Run tests and update snapshots
cargo insta review

# Run a specific test
cargo test -p pgt_pretty_print test_single
```

### Test Validation

Each test validates:

1. **Line Length**: No line exceeds `max_line_length` (except for string literals)
2. **AST Equality**: Parsing the formatted output produces the same AST as the original
3. **Snapshot Match**: Output matches the stored snapshot

### Adding New Tests

You can and should create new test cases to validate your implementations!

1. **Create test file**:
   ```bash
   # For single statement tests
   echo "SELECT * FROM users WHERE age > 18" > tests/data/single/user_query_80.sql

   # For multi-statement tests
   cat > tests/data/multi/example_queries_60.sql <<'EOF'
   SELECT id FROM users;
   INSERT INTO logs (message) VALUES ('test');
   EOF
   ```

2. **Naming convention**: `<descriptive_name>_<line_length>.sql`
   - The number at the end is the max line length (e.g., `60`, `80`, `120`)
   - Examples: `complex_join_80.sql`, `insert_with_cte_60.sql`

3. **Run specific test**:
   ```bash
   # Run single test with output
   cargo test -p pgt_pretty_print test_single__user_query_80 -- --show-output

   # Run all tests matching pattern
   cargo test -p pgt_pretty_print test_single -- --show-output
   ```

4. **Review snapshots**:
   ```bash
   # Generate/update snapshots
   cargo insta review

   # Accept all new snapshots
   cargo insta accept
   ```

5. **Iterate**: Adjust your `emit_*` implementation based on test output

## Feedback Loop

### Development Workflow

1. **Identify a Node Type**
   - Look at test failures to see which node types are unimplemented
   - Check `src/nodes/mod.rs` for the `todo!()` in `emit_node_enum`

2. **Study the Go Implementation and pgFormatter**
   - Find the corresponding node in `parser/ast/*.go`
   - Study its `SqlString()` method for SQL structure
   - Use pgFormatter for line breaking ideas: `pg_format tests/data/single/your_test.sql`
   - Understand the structure and formatting rules

3. **Create Rust Implementation**
   - Create new file: `src/nodes/<node_name>.rs`
   - Implement `emit_<node_name>` function
   - Add to `mod.rs` imports and dispatch

4. **Test and Iterate**
   ```bash
   # Run tests to see if implementation works
   cargo test -p pgt_pretty_print

   # Review snapshots
   cargo insta review

   # Check specific test output
   cargo test -p pgt_pretty_print -- <test_name> --nocapture
   ```

5. **Refine Layout**
   - Adjust group boundaries for better breaking behavior
   - Use `SoftOrSpace` for clauses that can stay on one line
   - Use `Soft` for items that should prefer breaking
   - Add indentation for nested structures

### Debugging Tips

1. **Compare Snapshots**: Use `cargo insta review` to see diffs

2. **Check Parsed AST**: All tests print both old and new content as well as the old AST. If ASTs do not match, they show both. Run the tests with `-- --show-output` to see the stdout. This will help to see if an emit function misses a few properties of the node.

## Key Patterns and Best Practices

### 1. Group Boundaries

Groups determine where the renderer can break lines. Good practices:

- **Statement-level groups**: Wrap entire statements (SELECT, INSERT, etc.)
- **Clause-level groups**: Each clause (FROM, WHERE, ORDER BY) in a group
- **Expression-level groups**: Function calls, case expressions, parenthesized expressions

### 2. Line Break Strategy

- **After major keywords**: `SELECT`, `FROM`, `WHERE`, `ORDER BY`
  - Use `LineType::SoftOrSpace` to allow single-line for short queries
- **Between list items**: Comma-separated lists
  - Use `LineType::SoftOrSpace` after commas
- **Around operators**: Binary operators in expressions
  - Generally use spaces, not line breaks (handled by groups)

### 3. Indentation

- **Start indent**: After major keywords that introduce multi-item sections
  ```rust
  e.token(TokenKind::SELECT_KW);
  e.indent_start();
  e.line(LineType::SoftOrSpace);
  emit_comma_separated_list(e, &n.target_list, super::emit_node);
  e.indent_end();
  ```

- **Nested structures**: Subqueries, CASE expressions, function arguments

### 4. Whitespace Handling

- **Space before/after**: Most keywords and operators need spaces
- **No space**: Between qualifiers (`schema.table`, `table.column`)
- **Conditional space**: Use groups to let renderer decide

### 5. Special Cases

- **Parentheses**: Always emit as tokens, group contents
  ```rust
  e.token(TokenKind::LPAREN);
  e.group_start(GroupKind::ParenExpr);
  super::emit_node(&n.expr, e);
  e.group_end();
  e.token(TokenKind::RPAREN);
  ```

- **String literals**: Emit as tokens (no formatting inside)
- **Identifiers**: May need quoting (handled in token rendering)
- **Operators**: Can be keywords (`AND`) or symbols (`+`, `=`)

## Node Coverage Checklist

**Total Nodes**: ~270 node types from `pgt_query::protobuf::NodeEnum`

### Implementation Approach

**You can implement nodes partially!** For complex nodes with many fields:
1. Implement basic/common fields first
2. Add `todo!()` or comments for unimplemented parts
3. Test with simple cases
4. Iterate and add more fields as needed

Example partial implementation:
```rust
pub(super) fn emit_select_stmt(e: &mut EventEmitter, n: &SelectStmt) {
    e.group_start(GroupKind::SelectStmt);

    e.token(TokenKind::SELECT_KW);
    // Emit target list
    // TODO: DISTINCT clause
    // TODO: Window clause
    // TODO: GROUP BY
    // TODO: HAVING
    // TODO: ORDER BY
    // TODO: LIMIT/OFFSET

    e.group_end();
}
```

### Completed Nodes (14/270)
- [x] AConst (with all variants: Integer, Float, Boolean, String, BitString)
- [x] AExpr (partial - basic binary operators)
- [x] AStar
- [x] BitString
- [x] Boolean
- [x] BoolExpr (AND/OR/NOT)
- [x] ColumnRef
- [x] Float
- [x] Integer
- [x] RangeVar (partial - schema.table)
- [x] ResTarget (partial - SELECT and UPDATE SET contexts)
- [x] SelectStmt (partial - basic SELECT FROM WHERE)
- [x] String (identifier and literal contexts)
- [x] UpdateStmt (partial - UPDATE table SET col = val WHERE)

## 📚 Implementation Learnings & Session Notes

**Update this section as you implement nodes!** Document patterns, gotchas, edge cases, and decisions made during implementation.

### Session Log Format

For each work session, add an entry with:
- **Date**: When the work was done
- **Nodes Implemented**: Which nodes were added/modified
- **Progress**: Updated node count
- **Learnings**: Key insights, patterns discovered, problems solved
- **Next Steps**: What to tackle next

---

### Example Entry (Template - Replace with actual sessions)

**Date**: 2025-01-15
**Nodes Implemented**: InsertStmt, DeleteStmt
**Progress**: 14/270 → 16/270

**Learnings**:
- InsertStmt has multiple variants (VALUES, SELECT, DEFAULT VALUES)
- Use `assert_node_variant!` for SELECT subqueries in INSERT
- OnConflictClause is optional and complex - implemented basic DO NOTHING first
- pgFormatter breaks INSERT after column list - used `SoftOrSpace` after closing paren

**Challenges**:
- InsertStmt.select_stmt can be SelectStmt or other query types - handled with generic emit_node
- Column list formatting needed custom helper function

**Next Steps**:
- Complete OnConflictClause (DO UPDATE variant)
- Implement CreateStmt for table definitions
- Add more INSERT test cases with CTEs

---

### Work Session Notes (Add entries below)

<!-- Add new session entries here as you implement nodes -->

---

### Priority Groups & Node Categories

**High Priority (~50 nodes)**: Core DML/DDL, Essential Expressions, JOINs, CTEs
- InsertStmt, DeleteStmt, CreateStmt, DropStmt, TruncateStmt
- FuncCall, TypeCast, CaseExpr, NullTest, SubLink, AArrayExpr
- JoinExpr, WithClause, CommonTableExpr, SortBy, WindowDef
- ColumnDef, Constraint, TypeName, OnConflictClause

**Medium Priority (~100 nodes)**: Range refs, Set ops, Additional statements
- RangeSubselect, RangeFunction, Alias, SetOperationStmt
- CreateSchemaStmt, GrantStmt, TransactionStmt, CopyStmt, IndexStmt
- 30+ Alter statements, 30+ Create statements

**Lower Priority (~100 nodes)**: JSON/XML, Internal nodes, Specialized
- 30+ Json* nodes, XmlExpr, Query, RangeTblEntry, TargetEntry
- Replication, Subscriptions, Type coercion nodes

**Complete alphabetical list** (270 nodes): See `crates/pgt_query/src/protobuf.rs` `node::Node` enum for full list

## Code Generation

The project uses procedural macros for code generation:

- **TokenKind**: Generated from keywords and operators
- **GroupKind**: Generated for each node type

If you need to add new tokens or groups:

1. Check if code generation is needed (usually not for individual nodes)
2. Tokens are likely already defined for all SQL keywords
3. Groups are auto-generated based on node types

## References

### Key Files
- `src/nodes/mod.rs`: Central dispatch for all node types
- `src/nodes/select_stmt.rs`: Example of complex statement
- `src/nodes/a_expr.rs`: Example of expression handling
- `src/nodes/node_list.rs`: List helper functions
- `parser/ast/statements.go`: Go reference for statements
- `parser/ast/expressions.go`: Go reference for expressions

### Useful Commands
```bash
# Run formatter on all code
just format

# Run all tests
just test

# Run specific crate tests
cargo test -p pgt_pretty_print

# Update test snapshots
cargo insta review

# Run clippy
just lint

# Check if ready to commit
just ready
```

## Next Steps

1. **Review this plan** and adjust as needed
2. **Start with high-priority nodes**: Focus on DML statements (INSERT, DELETE) and essential expressions (FuncCall, TypeCast, etc.)
3. **Use test-driven development**:
   - Create a test case for the SQL you want to format
   - Run: `cargo test -p pgt_pretty_print test_single__<your_test> -- --show-output`
   - Implement the `emit_*` function
   - Iterate based on test output
4. **Implement partially**: Don't try to handle all fields at once - start with common cases
5. **Iterate progressively**: Add more fields and edge cases as you go

## Summary: Key Points

### ✅ DO:
- **Implement `emit_*` functions** for AST nodes in `src/nodes/`
- **Create test cases** to validate your implementations
- **Run specific tests** with `cargo test -p pgt_pretty_print test_single__<name> -- --show-output`
- **Implement nodes partially** - handle common fields first, add TODOs for the rest
- **Use Go parser** as reference for SQL generation logic
- **Use pgFormatter for inspiration** on line breaking: `pg_format tests/data/single/your_test.sql`
- **Use existing helpers** from `node_list.rs` for lists
- **Use `assert_node_variant!`** to extract specific node types from generic Nodes
- **⚠️ UPDATE THIS DOCUMENT** after each session:
  - Mark nodes as `[x]` in "Completed Nodes"
  - Add entry to "Implementation Learnings & Session Notes"
  - Update progress count

### ❌ DON'T:
- **Don't modify** `src/renderer.rs` (layout engine - complete)
- **Don't modify** `src/emitter.rs` (event emitter - complete)
- **Don't modify** `tests/tests.rs` (test infrastructure - complete)
- **Don't modify** `src/codegen/` (code generation - complete)
- **Don't try to implement everything at once** - partial implementations are fine!

### 🎯 Goals:
- **~270 total nodes** to eventually implement
- **~14 nodes** currently done
- **~50 high-priority nodes** should be tackled first
- **Each node** can be implemented incrementally
- **Tests validate** both correctness (AST equality) and formatting (line length)

## Notes

- The pretty printer is **structure-preserving**: it should not change the AST
- The formatter is **line-length-aware**: it respects `max_line_length` when possible
- String literals and JSON content may exceed line length (allowed by tests)
- The renderer uses a **greedy algorithm**: tries single-line first, then breaks
- Groups enable **local layout decisions**: inner groups can break independently

## Quick Reference: Adding a New Node

Follow these steps to implement a new AST node:

### 1. Create the file

```bash
# Create new file in src/nodes/
touch src/nodes/<node_name>.rs
```

### 2. Implement the emit function

```rust
// src/nodes/<node_name>.rs
use pgt_query::protobuf::<NodeType>;
use crate::{TokenKind, emitter::{EventEmitter, GroupKind}};

pub(super) fn emit_<node_name>(e: &mut EventEmitter, n: &<NodeType>) {
    e.group_start(GroupKind::<NodeName>);

    // Emit tokens, spaces, and child nodes
    e.token(TokenKind::KEYWORD_KW);
    e.space();
    // ... implement based on Go SqlString() method

    e.group_end();
}
```

### 3. Register in mod.rs

```rust
// src/nodes/mod.rs

// Add module declaration
mod <node_name>;

// Add import
use <node_name>::emit_<node_name>;

// Add to dispatch in emit_node_enum()
pub fn emit_node_enum(node: &NodeEnum, e: &mut EventEmitter) {
    match &node {
        // ... existing cases
        NodeEnum::<NodeName>(n) => emit_<node_name>(e, n),
        // ...
    }
}
```

### 4. Test

```bash
# Run tests to see if it works
cargo test -p pgt_pretty_print

# Review snapshot output
cargo insta review
```

### 5. Iterate

- Check Go implementation in `parser/ast/*.go` for reference
- Adjust groups, spaces, and line breaks based on test output
- Ensure AST equality check passes (tests validate this automatically)

## Files You'll Work With

**Primary files** (where you implement):
- `src/nodes/mod.rs` - Register new nodes here
- `src/nodes/<node_name>.rs` - Implement each node here
- `src/nodes/node_list.rs` - Helper functions (read-only, may add helpers)
- `src/nodes/string.rs` - String/identifier helpers (read-only)

**Reference files** (read for examples):
- `src/nodes/select_stmt.rs` - Complex statement example
- `src/nodes/update_stmt.rs` - Example with `assert_node_variant!`
- `src/nodes/res_target.rs` - Example with multiple emit functions
- `src/nodes/range_var.rs` - Simple node example
- `src/nodes/column_ref.rs` - List helper example

**Go reference files** (read for SQL logic):
- `parser/ast/statements.go` - Main SQL statements
- `parser/ast/expressions.go` - Expression nodes
- `parser/ast/ddl_statements.go` - DDL statements
- Other `parser/ast/*.go` files as needed

**DO NOT MODIFY**:
- `src/renderer.rs` - Layout engine (already complete)
- `src/emitter.rs` - Event emitter (already complete)
- `src/codegen/` - Code generation (already complete)
- `tests/tests.rs` - Test infrastructure (already complete)
