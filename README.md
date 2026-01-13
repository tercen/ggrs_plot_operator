# ggrs_plot_operator
Plot operator using ggrs

# Roadmap

### 0.0.1

- [x] Data load
- [x] Streaming and chunking architecture
- [x] Scatter plot with a single facet
- [x] GITHUB Actions (CI)
- [x] GITHUB Actions (Release)
- [x] Plot saving


### 0.0.2

- [ ] Scatter plot with multiple facets (row/column/grid faceting with FreeY scales)
- [x] Optimize bulk streaming for multi-facet (currently uses per-facet chunking)
- [ ] Add operator properties - Plot width and height
- [ ] Review and optimize dependencies

### 0.0.3

- [ ] Support for minimal and white themes
- [ ] Add support for colors
- [ ] Add plot legend

### 0.0.4

- [ ] Add bar plot
- [ ] Add line plot
- [ ] Add support for manual axis ranges

### 0.0.5

- [ ] Add heatmap
- [ ] Configurable textual elements in plot (axis labels, legend, title)

### Unspecified Version
- [ ] Switching between GPU / CPU