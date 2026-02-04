---
name: review_code
description: Perform a comprehensive code review identifying architecture principle violations and outdated files. Use when asked to review code quality, find violations, or audit the codebase.
disable-model-invocation: true
---

# Review Code Skill

Perform a comprehensive code review of this project and the related ggrs repository, identifying violations of architecture principles and outdated files.

**Note**: Project rules (`.claude/rules/*.md`) are automatically loaded. Use them as the reference for what constitutes a violation.

## Scope

Review both repositories:
- **ggrs_plot_operator**: Current directory (`src/`)
- **ggrs-core**: `../ggrs/crates/ggrs-core/src/`

## What to Check

### 1. Architecture Principle Violations

Review code for violations of the loaded architecture principles:

#### Separation of Concerns
- Structs/modules handling multiple disparate functionalities
- Mixed responsibilities (e.g., data fetching + transformation + rendering in one place)
- Functions doing too many things

#### Abstraction Hierarchies
- Missing trait abstractions where polymorphism would help
- Duplicated code that should be centralized
- Concrete dependencies where abstractions should be used

#### Fallback Violations
- `unwrap_or_default()` usage (unless explicitly justified)
- `unwrap_or()` with fallback values
- If-else patterns that silently handle missing data
- Any "recovery" logic that masks errors

#### Error Handling Violations
- Errors being swallowed (logged but not propagated)
- Missing error context (bare `.unwrap()` or `?` without `.map_err()`)
- Silent failures

#### Columnar Architecture Violations
- Row-by-row iteration over DataFrames
- Building Vec<Record> instead of columnar operations

#### Coupling Issues
- Direct dependencies on concrete types where traits exist
- Tight coupling between unrelated modules

### 2. Outdated Files

Identify files that may be obsolete:
- Session files (`SESSION_*.md`) older than 2 weeks
- Documentation that references removed/renamed code
- Dead code (unused functions, structs, modules)
- Commented-out code blocks
- TODO/FIXME comments that are stale
- Test files for removed functionality

## Output Format

Create a review document at `REVIEW_REPORT.md` with the following structure:

```markdown
# Code Review Report

**Date**: YYYY-MM-DD
**Repositories Reviewed**: ggrs_plot_operator, ggrs-core

## Executive Summary

[2-3 sentence overview of findings]

## Principle Violations

### Separation of Concerns
| File | Line(s) | Issue |
|------|---------|-------|
| path/to/file.rs | 123-145 | Brief description |

### Fallback/Recovery Violations
| File | Line(s) | Issue |
|------|---------|-------|

### Error Handling Issues
| File | Line(s) | Issue |
|------|---------|-------|

### Abstraction Issues
| File | Line(s) | Issue |
|------|---------|-------|

### Coupling Issues
| File | Line(s) | Issue |
|------|---------|-------|

### Other Issues
| File | Line(s) | Issue |
|------|---------|-------|

## Outdated Files

### Candidates for Deletion
| File | Reason |
|------|--------|

### Candidates for Update
| File | Reason |
|------|--------|

## Statistics

- Total violations found: N
- Critical violations: N
- Files reviewed: N
- Outdated files identified: N
```

## Instructions

1. **Do NOT propose fixes** - Only identify and document issues
2. **Be specific** - Include file paths and line numbers
3. **Be concise** - Brief descriptions, not detailed explanations
4. **Focus on principles** - Reference the specific principle violated
5. **Prioritize** - Note critical issues vs minor issues
6. **Check both repos** - Review ggrs_plot_operator AND ggrs-core

## Execution

1. Systematically review source files in `src/`
2. Review ggrs-core at `../ggrs/crates/ggrs-core/src/`
3. Check for outdated files (SESSION_*.md, old docs, dead code)
4. Write findings to `REVIEW_REPORT.md`
5. Report completion with summary statistics
