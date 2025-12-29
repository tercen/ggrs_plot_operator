# GGRS Plot Operator - Implementation Plan

## Project Phases

This document outlines the implementation roadmap for the GGRS Plot Operator, broken down into manageable phases with clear deliverables.

---

## Phase 0: Project Setup & Infrastructure

**Goal**: Set up the project structure, build system, and development environment

### Tasks

1. **Project Initialization**
   - [ ] Initialize Rust project with Cargo
   - [ ] Create workspace structure
   - [ ] Set up `.gitignore` for Rust/Tercen artifacts
   - [ ] Create `README.md` with project overview
   - [ ] Set up `LICENSE` file (MIT/Apache-2.0 dual license)

2. **Proto Integration**
   - [ ] Copy/link Tercen proto files from `../sci/tercen_grpc/tercen_grpc_api/protos/`
   - [ ] Create `build.rs` for proto compilation
   - [ ] Add `tonic` and `prost` dependencies
   - [ ] Verify proto compilation works
   - [ ] Generate Rust types from protos

3. **Docker Setup**
   - [ ] Create multi-stage `Dockerfile`
   - [ ] Create `.dockerignore`
   - [ ] Test local Docker build
   - [ ] Set up GitHub Container Registry publishing

4. **Development Environment**
   - [ ] Create `rustfmt.toml` for code formatting
   - [ ] Create `clippy.toml` for linting rules
   - [ ] Set up VS Code/IDE configuration (`.vscode/`)
   - [ ] Create development Docker Compose for local Tercen instance

5. **CI/CD Pipeline**
   - [ ] GitHub Actions for Rust build
   - [ ] GitHub Actions for tests
   - [ ] GitHub Actions for Docker build & push
   - [ ] Dependency update automation (Dependabot)

**Deliverable**: Working project skeleton with build system

**Estimated Effort**: 2-3 days

---

## Phase 1: gRPC Client Foundation

**Goal**: Establish connection to Tercen and implement basic gRPC communication

### Tasks

1. **Authentication Module**
   ```rust
   // src/auth.rs
   ```
   - [ ] Implement token-based authentication
   - [ ] Token storage and refresh logic
   - [ ] Handle authentication errors
   - [ ] Unit tests for auth module

2. **gRPC Client Setup**
   ```rust
   // src/grpc_client.rs
   ```
   - [ ] Create `TercenGrpcClient` struct
   - [ ] Establish gRPC channel with TLS
   - [ ] Implement connection pooling
   - [ ] Add retry logic with exponential backoff
   - [ ] Handle connection errors

3. **Service Clients**
   ```rust
   // src/services/task_service.rs
   // src/services/table_service.rs
   // src/services/file_service.rs
   ```
   - [ ] Wrap `TaskService` client
   - [ ] Wrap `TableSchemaService` client
   - [ ] Wrap `FileService` client
   - [ ] Integration tests with mock Tercen instance

4. **Configuration**
   ```rust
   // src/config.rs
   ```
   - [ ] Parse operator config from environment/args
   - [ ] Define `OperatorConfig` struct
   - [ ] Validation logic
   - [ ] Unit tests

**Deliverable**: Working gRPC client that can authenticate and communicate with Tercen

**Estimated Effort**: 4-5 days

**Testing**:
- Unit tests for each module
- Integration tests with local Tercen instance
- Mock gRPC server for CI tests

---

## Phase 2: Data Retrieval & Transformation

**Goal**: Fetch data from Tercen and convert to GGRS-compatible formats

### Tasks

1. **Data Streaming Module**
   ```rust
   // src/data/stream.rs
   ```
   - [ ] Implement chunked data retrieval via `streamTable()`
   - [ ] Parse CSV format responses
   - [ ] Parse binary format responses (Arrow)
   - [ ] Handle streaming errors and retries
   - [ ] Tests with sample data

2. **DataFrame Builder**
   ```rust
   // src/data/dataframe.rs
   ```
   - [ ] Convert streamed data to GGRS DataFrame
   - [ ] Handle type conversions (Tercen → GGRS Value types)
   - [ ] Column name mapping
   - [ ] Missing value handling
   - [ ] Tests with various data types

3. **Aesthetic Mapper**
   ```rust
   // src/data/aes_mapper.rs
   ```
   - [ ] Parse Tercen crosstab specification
   - [ ] Map X/Y axes to GGRS Aes
   - [ ] Map color factors to GGRS Aes
   - [ ] Map shape/size factors to GGRS Aes
   - [ ] Handle multi-factor aesthetics
   - [ ] Tests with complex mappings

4. **Facet Mapper**
   ```rust
   // src/data/facet_mapper.rs
   ```
   - [ ] Map Tercen row factors to GGRS FacetSpec
   - [ ] Map Tercen column factors to GGRS FacetSpec
   - [ ] Handle grid faceting (row + column)
   - [ ] Map scales mode (fixed/free/free_x/free_y)
   - [ ] Tests for all facet combinations

5. **Data Cache & Optimization**
   ```rust
   // src/data/cache.rs
   ```
   - [ ] Implement simple in-memory cache
   - [ ] Cache invalidation strategy
   - [ ] Memory usage monitoring
   - [ ] Benchmark caching performance

**Deliverable**: Complete data pipeline from Tercen to GGRS DataFrame

**Estimated Effort**: 5-6 days

**Testing**:
- Unit tests for each transformer
- Integration tests with real Tercen data
- Performance benchmarks for large datasets

---

## Phase 3: GGRS Integration

**Goal**: Implement custom StreamGenerator and integrate with GGRS

### Tasks

1. **TercenStreamGenerator Implementation**
   ```rust
   // src/ggrs_integration/stream_generator.rs
   ```
   - [ ] Implement `StreamGenerator` trait
   - [ ] Implement `query_cell_data()` with lazy loading
   - [ ] Implement `get_facet_spec()` and `get_aes()`
   - [ ] Handle facet cell-specific queries
   - [ ] Memory-efficient data management
   - [ ] Tests with mock data

2. **Plot Configuration Builder**
   ```rust
   // src/ggrs_integration/plot_builder.rs
   ```
   - [ ] Build `EnginePlotSpec` from operator config
   - [ ] Map geometry types (point, line, bar)
   - [ ] Map theme settings
   - [ ] Handle titles, labels, legends
   - [ ] Apply custom styling
   - [ ] Tests for all configurations

3. **Plot Generator Wrapper**
   ```rust
   // src/ggrs_integration/generator.rs
   ```
   - [ ] Create and configure `PlotGenerator`
   - [ ] Handle GGRS errors
   - [ ] Progress reporting during generation
   - [ ] Tests with various plot types

4. **Image Renderer Wrapper**
   ```rust
   // src/ggrs_integration/renderer.rs
   ```
   - [ ] Configure `ImageRenderer` with dimensions
   - [ ] Render to PNG buffer
   - [ ] Handle rendering errors
   - [ ] Memory cleanup after rendering
   - [ ] Tests for different dimensions

**Deliverable**: Working integration between Tercen data and GGRS plotting

**Estimated Effort**: 4-5 days

**Testing**:
- Unit tests for each component
- Visual regression tests (compare with expected PNGs)
- Performance benchmarks

---

## Phase 4: Result Upload & Task Management

**Goal**: Upload generated plots to Tercen and manage task lifecycle

### Tasks

1. **File Upload Module**
   ```rust
   // src/upload/file_uploader.rs
   ```
   - [ ] Create `EFileDocument` from PNG buffer
   - [ ] Implement streaming upload via `FileService.upload()`
   - [ ] Handle upload chunks
   - [ ] Retry logic for failed uploads
   - [ ] Tests with mock file service

2. **Task Manager**
   ```rust
   // src/task/manager.rs
   ```
   - [ ] Task initialization from TaskService
   - [ ] Parse ComputationTask details
   - [ ] Update task state (Running → Done/Failed)
   - [ ] Send TaskProgressEvent updates
   - [ ] Send TaskLogEvent messages
   - [ ] Tests for task lifecycle

3. **Result Linking**
   ```rust
   // src/task/result_linker.rs
   ```
   - [ ] Link uploaded file to computation result
   - [ ] Handle multiple output files (pages)
   - [ ] Update result metadata
   - [ ] Tests with mock services

4. **Progress Reporting**
   ```rust
   // src/task/progress.rs
   ```
   - [ ] Report data loading progress
   - [ ] Report plot generation progress
   - [ ] Report upload progress
   - [ ] Aggregate progress across facets
   - [ ] Tests for progress calculation

**Deliverable**: Complete task execution with result upload

**Estimated Effort**: 3-4 days

**Testing**:
- Integration tests with Tercen
- End-to-end tests with real tasks

---

## Phase 5: Main Application & Error Handling

**Goal**: Implement main execution loop and robust error handling

### Tasks

1. **Main Application Entry**
   ```rust
   // src/main.rs
   ```
   - [ ] Parse command-line arguments
   - [ ] Initialize logging (tracing)
   - [ ] Load configuration
   - [ ] Initialize gRPC client
   - [ ] Start task polling loop
   - [ ] Graceful shutdown handling

2. **Operator Executor**
   ```rust
   // src/executor.rs
   ```
   - [ ] Orchestrate full execution pipeline:
     1. Receive task
     2. Fetch data
     3. Transform data
     4. Generate plot
     5. Upload result
     6. Update task
   - [ ] Implement retry logic
   - [ ] Handle partial failures
   - [ ] Cleanup resources

3. **Error Handling**
   ```rust
   // src/error.rs
   ```
   - [ ] Define comprehensive `OperatorError` enum
   - [ ] Implement `From` conversions for all error types
   - [ ] User-friendly error messages
   - [ ] Error logging and reporting
   - [ ] Tests for error scenarios

4. **Logging & Observability**
   ```rust
   // src/observability.rs
   ```
   - [ ] Configure `tracing` subscriber
   - [ ] Structured logging for key events
   - [ ] Performance metrics collection
   - [ ] Health check endpoint (optional)

**Deliverable**: Fully functional operator binary

**Estimated Effort**: 3-4 days

**Testing**:
- End-to-end tests
- Error scenario tests
- Performance stress tests

---

## Phase 6: Operator Configuration & Properties

**Goal**: Support all plot customization properties

### Tasks

1. **operator.json Definition**
   ```json
   // operator.json
   ```
   - [ ] Define operator metadata
   - [ ] Define all properties (theme, dimensions, scales, etc.)
   - [ ] Define input specs (crosstab mapping)
   - [ ] Define output specs
   - [ ] Validate against Tercen schema

2. **Property Parsing**
   ```rust
   // src/config/properties.rs
   ```
   - [ ] Parse all EnumeratedProperty values
   - [ ] Parse all StringProperty values
   - [ ] Parse all DoubleProperty values
   - [ ] Parse all BooleanProperty values
   - [ ] Validation and defaults
   - [ ] Tests for all properties

3. **Theme Support**
   - [ ] Map Tercen theme names to GGRS themes
   - [ ] Support custom theme properties
   - [ ] Implement theme presets (gray, bw, minimal, etc.)
   - [ ] Tests for each theme

4. **Geometry Type Support**
   - [ ] Detect plot type from data/config
   - [ ] Support point plots
   - [ ] Support line plots (future)
   - [ ] Support bar plots (future)
   - [ ] Support heatmaps (future)
   - [ ] Tests for each geometry

5. **Advanced Features**
   - [ ] Split cells mode (multiple output files)
   - [ ] Page factors (separate files per page)
   - [ ] Custom axis ranges
   - [ ] Legend customization
   - [ ] Axis label rotation
   - [ ] Tests for advanced features

**Deliverable**: Full-featured operator with all customization options

**Estimated Effort**: 5-6 days

**Testing**:
- Configuration validation tests
- Visual tests for each property
- Comparison with R plot_operator output

---

## Phase 7: Optimization & Performance

**Goal**: Optimize for production use with large datasets

### Tasks

1. **Memory Optimization**
   - [ ] Profile memory usage
   - [ ] Implement streaming for large data
   - [ ] Optimize DataFrame construction
   - [ ] Minimize allocations in hot paths
   - [ ] Add memory limits and safeguards

2. **Performance Benchmarking**
   - [ ] Create benchmark suite
   - [ ] Benchmark data retrieval
   - [ ] Benchmark plot generation
   - [ ] Benchmark file upload
   - [ ] Compare with R plot_operator

3. **Parallel Processing**
   - [ ] Identify parallelizable operations
   - [ ] Parallel facet rendering (if GGRS supports)
   - [ ] Parallel data chunk processing
   - [ ] Benchmark parallel vs sequential

4. **Caching Strategy**
   - [ ] Implement smart data caching
   - [ ] Cache aesthetic computations
   - [ ] Cache scale training results
   - [ ] Measure cache hit rates

5. **Resource Management**
   - [ ] Set up proper resource limits
   - [ ] Implement circuit breakers for failures
   - [ ] Add timeouts for long operations
   - [ ] Graceful degradation strategies

**Deliverable**: Production-ready operator with optimized performance

**Estimated Effort**: 4-5 days

**Testing**:
- Performance regression tests
- Large dataset stress tests
- Memory leak tests

---

## Phase 8: Testing & Documentation

**Goal**: Comprehensive testing and user documentation

### Tasks

1. **Unit Tests**
   - [ ] Achieve >80% code coverage
   - [ ] Test all error paths
   - [ ] Test edge cases
   - [ ] Mock external dependencies

2. **Integration Tests**
   - [ ] End-to-end tests with local Tercen
   - [ ] Test all supported plot types
   - [ ] Test all property combinations
   - [ ] Test error recovery

3. **Visual Regression Tests**
   - [ ] Set up visual testing framework
   - [ ] Create reference plots
   - [ ] Automated comparison with GGRS examples
   - [ ] Comparison with R plot_operator output

4. **Performance Tests**
   - [ ] Benchmark suite for CI
   - [ ] Test with datasets of varying sizes
   - [ ] Test with different facet configurations
   - [ ] Generate performance reports

5. **User Documentation**
   - [ ] Update README with usage instructions
   - [ ] Create user guide with examples
   - [ ] Document all properties
   - [ ] Create troubleshooting guide

6. **Developer Documentation**
   - [ ] Code documentation (rustdoc)
   - [ ] Architecture documentation (update this doc)
   - [ ] Contribution guidelines
   - [ ] API reference

**Deliverable**: Well-tested and documented operator

**Estimated Effort**: 5-6 days

---

## Phase 9: Deployment & Release

**Goal**: Deploy operator to production and establish release process

### Tasks

1. **Docker Optimization**
   - [ ] Multi-stage build optimization
   - [ ] Minimize image size
   - [ ] Security scanning
   - [ ] Layer caching optimization

2. **Container Registry Setup**
   - [ ] Push to GitHub Container Registry
   - [ ] Set up image tagging strategy
   - [ ] Implement semantic versioning
   - [ ] Create release automation

3. **Tercen Integration**
   - [ ] Register operator in Tercen registry
   - [ ] Test in staging environment
   - [ ] User acceptance testing
   - [ ] Deploy to production

4. **Monitoring & Logging**
   - [ ] Set up log aggregation
   - [ ] Configure error alerting
   - [ ] Set up performance monitoring
   - [ ] Create dashboards

5. **Release Process**
   - [ ] Create release checklist
   - [ ] Write release notes
   - [ ] Tag v0.1.0 release
   - [ ] Announce to users

**Deliverable**: Production deployment with monitoring

**Estimated Effort**: 3-4 days

---

## Phase 10: Future Enhancements (Post-MVP)

**Goal**: Add advanced features and improvements

### Potential Features

1. **Additional Output Formats**
   - [ ] PDF export support
   - [ ] SVG export support
   - [ ] Interactive HTML (GGRS-WASM)

2. **More Geometry Types**
   - [ ] Line plots
   - [ ] Bar plots with stacking
   - [ ] Area plots
   - [ ] Histogram
   - [ ] Box plots
   - [ ] Violin plots

3. **Advanced Aesthetics**
   - [ ] Size aesthetic
   - [ ] Alpha (transparency) aesthetic
   - [ ] Line type aesthetic
   - [ ] Multiple color scales

4. **Performance Improvements**
   - [ ] GPU acceleration (WebGPU backend)
   - [ ] Progressive rendering
   - [ ] Data pre-aggregation
   - [ ] Adaptive sampling for previews

5. **Custom Tercen Themes**
   - [ ] Match Tercen brand colors
   - [ ] Custom palette integration
   - [ ] User-defined themes

**Estimated Effort**: TBD based on priorities

---

## Dependencies Between Phases

```
Phase 0 (Setup)
    ↓
Phase 1 (gRPC Client) ←──────────┐
    ↓                            │
Phase 2 (Data Transform)         │
    ↓                            │
Phase 3 (GGRS Integration)       │
    ↓                            │
Phase 4 (Upload) ────────────────┘
    ↓
Phase 5 (Main App)
    ↓
Phase 6 (Config) ←───┐
    ↓                │
Phase 7 (Optimize) ──┤
    ↓                │
Phase 8 (Testing) ───┘
    ↓
Phase 9 (Deploy)
    ↓
Phase 10 (Future)
```

## Timeline Estimation

| Phase | Duration | Parallel Work Possible |
|-------|----------|------------------------|
| Phase 0 | 2-3 days | No |
| Phase 1 | 4-5 days | Partial (services) |
| Phase 2 | 5-6 days | Yes (modules) |
| Phase 3 | 4-5 days | Partial |
| Phase 4 | 3-4 days | No |
| Phase 5 | 3-4 days | No |
| Phase 6 | 5-6 days | Yes (properties) |
| Phase 7 | 4-5 days | Yes (benchmarks) |
| Phase 8 | 5-6 days | Yes (tests) |
| Phase 9 | 3-4 days | No |
| **Total** | **38-48 days** | **~25-35 days with parallelization** |

## Risk Assessment

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| GGRS API changes | High | Low | Pin specific GGRS version, monitor changes |
| Tercen API changes | High | Low | Use stable proto versions, version checking |
| Performance issues with large data | Medium | Medium | Early performance testing, streaming architecture |
| Memory limits in containers | Medium | Medium | Memory monitoring, configurable limits |
| Complex crosstab mappings | Medium | High | Start with simple cases, iterate |
| Color palette compatibility | Low | High | Document differences, provide mappings |
| Visual differences from ggplot2 | Low | Medium | Document known differences, prioritize common cases |

## Success Criteria

### MVP (Minimum Viable Product)

- [ ] Successfully executes point plot tasks from Tercen
- [ ] Supports basic faceting (row, column, grid)
- [ ] Generates PNG output
- [ ] Handles datasets up to 100K rows
- [ ] Supports basic themes (gray, bw, minimal)
- [ ] Error handling and logging
- [ ] Docker deployment
- [ ] User documentation

### Full Release (v1.0)

- [ ] All MVP criteria
- [ ] Support for multiple geometry types
- [ ] All operator properties implemented
- [ ] Performance benchmarks show <2x R operator runtime
- [ ] Memory usage <50% of R operator
- [ ] Visual equivalence to ggplot2 for common cases
- [ ] Comprehensive test coverage (>80%)
- [ ] Production monitoring and logging

## Development Guidelines

### Code Quality

- Follow Rust API guidelines
- Use `rustfmt` for formatting
- Pass `clippy` lints
- Write doc comments for public APIs
- Maintain test coverage >80%

### Git Workflow

- Feature branches for each phase/task
- Pull requests for all changes
- Peer review before merge
- Semantic commit messages
- Tag releases with semantic versioning

### Testing Strategy

- Write tests alongside implementation
- Test-driven development for complex logic
- Integration tests in separate directory
- Benchmarks for performance-critical code
- Visual regression tests for plots

### Documentation

- Update docs as code changes
- Keep architecture doc in sync
- Document design decisions
- Maintain changelog
- Write clear commit messages

## Getting Started

To begin implementation:

1. Review this plan and the architecture document
2. Set up development environment (Phase 0)
3. Create tracking issues for each phase in GitHub
4. Start with Phase 1 (gRPC Client Foundation)
5. Follow test-driven development practices
6. Update this document as plans evolve

## Questions to Resolve

Before starting implementation:

1. **Data Format**: Should we prioritize CSV or binary (Arrow) format for data streaming?
2. **Memory Limits**: What are the typical container memory limits in Tercen production?
3. **Tercen Version**: Which Tercen version should we target for compatibility?
4. **Authentication**: What is the exact token-based auth flow in production?
5. **Deployment**: Will we deploy to a private or public container registry?
6. **Performance Targets**: What are acceptable performance benchmarks vs. R operator?
7. **Priority Features**: Which geometry types are highest priority after point plots?
8. **Testing Infrastructure**: Do we have access to a staging Tercen instance for testing?

## Next Steps

1. Review this plan with stakeholders
2. Answer open questions
3. Create GitHub project with issues for each phase
4. Set up development environment
5. Begin Phase 0 implementation
6. Schedule regular progress reviews
