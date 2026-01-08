# Documentation Overview

This directory contains design documents, implementation notes, and session logs for the GGRS Plot Operator.

## 📚 Current Documentation (Read These First)

### Architecture & Design
- **[09_FINAL_DESIGN.md](09_FINAL_DESIGN.md)** ⭐⭐⭐ - **PRIMARY**: Complete architecture and final design decisions
- **[10_IMPLEMENTATION_PHASES.md](10_IMPLEMENTATION_PHASES.md)** - Phased implementation roadmap with phase completion status
- **[GPU_BACKEND_MEMORY.md](GPU_BACKEND_MEMORY.md)** - GPU rendering backend memory investigation and optimization (OpenGL vs Vulkan)

### Technical Details
- **[03_GRPC_INTEGRATION.md](03_GRPC_INTEGRATION.md)** - gRPC API specifications and integration patterns
- **[08_SIMPLE_STREAMING_DESIGN.md](08_SIMPLE_STREAMING_DESIGN.md)** - Streaming data architecture concepts

### Session Notes
- **[SESSION_2025-01-07.md](SESSION_2025-01-07.md)** - GPU backend memory optimization (OpenGL selection)
- **[SESSION_2025-01-05.md](SESSION_2025-01-05.md)** - Dequantization implementation and debugging

### Build & Deployment
- **[04_DOCKER_AND_CICD.md](04_DOCKER_AND_CICD.md)** - Docker and CI/CD pipeline details
- **[05_DOCKER_CICD_SUMMARY.md](05_DOCKER_CICD_SUMMARY.md)** - Summary of Docker/CI implementation

## 📁 Historical Documentation (Reference Only)

These documents represent earlier design iterations and are kept for historical context. **Do not use these for implementation** - refer to the current documentation above instead.

### Early Design Iterations
- **[01_ARCHITECTURE.md](01_ARCHITECTURE.md)** - Initial architecture sketch (superseded by 09_FINAL_DESIGN.md)
- **[02_IMPLEMENTATION_PLAN.md](02_IMPLEMENTATION_PLAN.md)** - Early implementation ideas (superseded by 10_IMPLEMENTATION_PHASES.md)

### Deprecated Approaches
- **[06_CONTEXT_DESIGN.md](06_CONTEXT_DESIGN.md)** - Python pattern analysis (TOO COMPLEX, not used)
- **[07_RUST_CONTEXT_IMPL.md](07_RUST_CONTEXT_IMPL.md)** - C# client-based design (TOO COMPLEX, not used)

**Why deprecated?** These documents explored creating a full OperatorContext abstraction similar to Python's approach. This was determined to be over-engineered for the streaming architecture we ultimately chose. The final design uses a simpler `TercenStreamGenerator` that directly implements GGRS's `StreamGenerator` trait.

## 🔍 Quick Reference

### I want to...

**...understand the overall architecture**
→ Read [09_FINAL_DESIGN.md](09_FINAL_DESIGN.md)

**...know what's been implemented**
→ Read [10_IMPLEMENTATION_PHASES.md](10_IMPLEMENTATION_PHASES.md)

**...understand GPU backend choices**
→ Read [GPU_BACKEND_MEMORY.md](GPU_BACKEND_MEMORY.md)

**...integrate with Tercen's gRPC API**
→ Read [03_GRPC_INTEGRATION.md](03_GRPC_INTEGRATION.md)

**...understand the streaming approach**
→ Read [08_SIMPLE_STREAMING_DESIGN.md](08_SIMPLE_STREAMING_DESIGN.md)

**...set up CI/CD and Docker**
→ Read [04_DOCKER_AND_CICD.md](04_DOCKER_AND_CICD.md)

**...see recent implementation work**
→ Read session notes: [SESSION_2025-01-07.md](SESSION_2025-01-07.md), [SESSION_2025-01-05.md](SESSION_2025-01-05.md)

## 📝 Documentation Standards

### When to Create New Documents

**Session Notes** (`SESSION_YYYY-MM-DD.md`):
- Created after significant development sessions
- Document problems encountered, solutions found, decisions made
- Include code changes, testing results, and lessons learned

**Design Documents** (`[NUMBER]_[TOPIC].md`):
- Created for major architectural decisions
- Should be comprehensive and self-contained
- Update existing docs rather than creating new ones when possible

**Investigation Reports** (`[TOPIC]_[TYPE].md`):
- Created when investigating specific issues (e.g., memory, performance)
- Include methodology, findings, and recommendations
- Examples: GPU_BACKEND_MEMORY.md

### Document Lifecycle

1. **Draft** - Initial ideas and sketches
2. **Active** - Currently relevant for implementation
3. **Historical** - Superseded but kept for reference
4. **Deprecated** - No longer applicable, marked as such in this README

## 🗂️ Related Documentation

Outside this directory:

- **`/CLAUDE.md`** - Project overview and development guide for Claude Code
- **`/BUILD.md`** - Build and deployment instructions
- **`/TEST_LOCAL.md`** - Local testing guide
- **`/WORKFLOW_TEST_INSTRUCTIONS.md`** - Workflow/step-based testing
- **`/IMPLEMENTATION_COMPLETE.md`** - Phase completion status
- **`/TESTING_STATUS.md`** - Testing phase status

## 🔄 Maintenance

This README should be updated when:
- New documentation is created
- Documents become superseded or deprecated
- The recommended reading order changes
- Major architectural decisions are made

Last updated: 2025-01-07
