# ggrs_plot_operator
Plot operator using ggrs

# Roadmap

### 0.0.1 ✅ COMPLETE

- [x] Data load
- [x] Streaming and chunking architecture
- [x] Scatter plot with a single facet
- [x] GITHUB Actions (CI)
- [x] GITHUB Actions (Release)
- [x] Plot saving


### 0.0.2 ✅ COMPLETE

- [x] Scatter plot with multiple facets (row/column/grid faceting with FreeY scales)
- [x] Optimize bulk streaming for multi-facet (currently uses per-facet chunking)
- [x] Add operator properties - Plot width/height with "auto", backend (cpu/gpu)
- [x] **Add support for continuous colors** (numeric color factors with palette interpolation)
- [x] Review and optimize dependencies

Note: Point size is hardcoded (4) - should come from crosstab model aesthetics.

### 0.0.3

- [x] Use operator input specs to get projection information
- [ ] Dynamic point size
- [x] Specify gRPC as communication protocol
- [x] Add pages
- [x] Add x axis support
- [x] Add support for continuous color scale 
- [x] Add support for categorical colors (ColorLevels column)
- [x] Add color scale legend
- [x] Configurable textual elements in plot (axis labels, legend, title)

Note: Legend positioning still requires fine-tuning

### 0.0.4

- [ ] Add heatmap
- [ ] Add Test context structure
- [ ] Support for minimal and white themes


### 0.0.5

- [ ] Add bar plot
- [ ] Add line plot
- [ ] Add support for manual axis ranges



### Unspecified Version
- [ ] Switching between GPU / CPU
- [ ] Further optimize bulk streaming for multi-facet