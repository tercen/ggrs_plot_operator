#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# setup_test_data.sh — Generate comprehensive showcase for GGRS Plot Operator.
#
# Creates a Tercen project, uploads test data, configures multiple scenarios
# (scatter, line, bar, heatmap × color variants × themes × backends),
# renders all combinations, and produces an interactive showcase.html.
#
# Usage: ./setup_test_data.sh
#
# Environment:
#   TERCEN_URI      Tercen REST URI (default: http://127.0.0.1:5400)
#   TERCEN_GRPC_URI gRPC URI (default: http://127.0.0.1:50051)
#   TERCEN_USER     Username (default: test)
#   TERCEN_PASSWORD Password (default: test)
# =============================================================================

TERCEN_URI="${TERCEN_URI:-http://127.0.0.1:5400}"
TERCEN_GRPC_URI="${TERCEN_GRPC_URI:-http://127.0.0.1:50051}"
TERCEN_USER="${TERCEN_USER:-test}"
TERCEN_PASSWORD="${TERCEN_PASSWORD:-test}"
OPERATOR_ID="b1bd09be60002836389b3285f21d3bcd"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREPARE_BIN="${SCRIPT_DIR}/target/dev-release/prepare"
DEV_BIN="${SCRIPT_DIR}/target/dev-release/dev"
OUTPUT_DIR="${SCRIPT_DIR}/showcase_output"
SHOWCASE_HTML="${SCRIPT_DIR}/showcase.html"

THEMES=(gray bw light dark minimal classic linedraw void publish)
HEATMAP_PALETTES=(Spectral Jet Viridis Hot Cool RdBu YlGnBu)

# Backends: override with SHOWCASE_BACKENDS env var (e.g., "cpu" for CI without GPU)
if [[ -n "${SHOWCASE_BACKENDS:-}" ]]; then
    IFS=' ' read -ra BACKENDS <<< "$SHOWCASE_BACKENDS"
else
    BACKENDS=(cpu gpu)
fi

# Axis transforms (currently only log, asinh, logicle are selectable in the Tercen UI)
TRANSFORMS=(none log asinh logicle)

# Toggle flags: text.disable, axis.lines.disable, grid.major.disable, grid.minor.disable
# Encoded as 4-digit binary suffix, e.g., _0000 = all shown, _1010 = text+major grid disabled
FLAGS_COMBOS=()
for t in 0 1; do for a in 0 1; do for gm in 0 1; do for gn in 0 1; do
    FLAGS_COMBOS+=("${t}${a}${gm}${gn}")
done; done; done; done

# Associative array: scenario_name → step_id
declare -A SCENARIO_STEPS

# ============================================================================
# Helpers
# ============================================================================

json_field() {
    grep "\"$1\"" | head -1 | sed 's/.*"'"$1"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/'
}

grpc_token() {
    tercenctl user create-token --validity 1h -f json 2>/dev/null | tr -d '"'
}

# ============================================================================
# 1. Check / install tercenctl
# ============================================================================

check_tercenctl() {
    if command -v tercenctl &>/dev/null; then
        echo "[OK] tercenctl found"
        return 0
    fi

    local candidates=(
        "${SCRIPT_DIR}/../sci/sci_ctl/tercenctl"
        "${SCRIPT_DIR}/../../sci/sci_ctl/tercenctl"
        "${HOME}/.local/bin/tercenctl"
        "/usr/local/bin/tercenctl"
    )

    local found=""
    for candidate in "${candidates[@]}"; do
        if [[ -x "$candidate" ]]; then
            found="$(cd "$(dirname "$candidate")" && pwd)/$(basename "$candidate")"
            break
        fi
    done

    if [[ -z "$found" ]]; then
        echo "[ERROR] tercenctl not found. Install it or add to PATH."
        exit 1
    fi

    local INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "$INSTALL_DIR"
    ln -sf "$found" "${INSTALL_DIR}/tercenctl"

    if command -v tercenctl &>/dev/null; then
        echo "[OK] tercenctl linked from $found"
    else
        echo "[ERROR] ${INSTALL_DIR} is not in PATH"
        exit 1
    fi
}

# ============================================================================
# 2. Ensure tercenctl context
# ============================================================================

ensure_context() {
    local current
    current=$(tercenctl context current 2>&1 || true)

    if echo "$current" | grep -q "No current context"; then
        tercenctl context add -n local -u "$TERCEN_URI" \
            --username "$TERCEN_USER" -p "$TERCEN_PASSWORD" --set-current
        echo "[OK] Context 'local' created"
    else
        echo "[OK] Using existing context"
    fi

    if ! tercenctl user whoami &>/dev/null; then
        echo "[ERROR] Authentication failed"
        exit 1
    fi
    echo "[OK] Authenticated as $(tercenctl user whoami -f json 2>/dev/null | json_field 'name')"
}

# ============================================================================
# 3. Generate test CSV
# ============================================================================

generate_csv() {
    local csv_path="$1"

    awk 'BEGIN {
        split("alpha,beta,gamma,delta", cat1, ",")
        split("red,green,blue,yellow", cat2, ",")
        split("small,medium,large,tiny,huge,normal", cat3, ",")

        # Per-CAT1 Y offsets: separates scatter/line groups visually
        y_off[1] = -6; y_off[2] = -2; y_off[3] = 2; y_off[4] = 6

        # Per-CAT1 slopes: each category has a different trend
        y_slope[1] = 0.3; y_slope[2] = 0.6; y_slope[3] = -0.2; y_slope[4] = 0.8

        # Distinct base means for each CAT1 x CAT2 cell (4x4 = 16 cells)
        # Spread across 0-100 range so heatmap shows clear structure
        base[1,1]=15; base[1,2]=85; base[1,3]=40; base[1,4]=60
        base[2,1]=70; base[2,2]=25; base[2,3]=90; base[2,4]=50
        base[3,1]=45; base[3,2]=55; base[3,3]=10; base[3,4]=75
        base[4,1]=80; base[4,2]=35; base[4,3]=65; base[4,4]=20

        print "x,y,heatval,CAT1,CAT2,CAT3,x_log,y_log,x_asinh,y_asinh,x_logicle,y_logicle"

        for (i = 1; i <= 5000; i++) {
            srand(i * 7 + 3)
            x = (rand() * 20) - 10

            c1_idx = ((i - 1) % 4) + 1
            c2_idx = (int((i - 1) / 4) % 4) + 1
            c1 = cat1[c1_idx]
            c2 = cat2[c2_idx]
            c3 = cat3[(int((i - 1) / 16) % 6) + 1]

            # y: category-dependent offset + slope + noise
            srand(i * 13 + 7)
            y = y_off[c1_idx] + (x * y_slope[c1_idx]) + (rand() * 4) - 2

            # heatval: category-dependent mean + noise, clamped to [0, 100]
            srand(i * 31 + 11)
            hv = base[c1_idx, c2_idx] + (rand() * 16) - 8
            if (hv < 0) hv = 0
            if (hv > 100) hv = 100

            # Pre-transformed columns for axis transform showcase
            # Log: ln(value), non-positive values become NaN (skipped by GGRS)
            x_log_s = (x > 0) ? sprintf("%.6f", log(x)) : ""
            y_log_s = (y > 0) ? sprintf("%.6f", log(y)) : ""

            # Asinh: handles negative values naturally
            # asinh(v) = ln(v + sqrt(v^2 + 1))
            x_asinh = log(x + sqrt(x*x + 1))
            y_asinh = log(y + sqrt(y*y + 1))

            # Logicle approximation: sign(v) * ln(1 + |v|)
            # Similar to logicle: linear near zero, log-like for large |v|
            ax = (x < 0) ? -x : x
            ay = (y < 0) ? -y : y
            sx = (x < 0) ? -1 : 1
            sy = (y < 0) ? -1 : 1
            x_logicle = sx * log(1 + ax)
            y_logicle = sy * log(1 + ay)

            printf "%.4f,%.4f,%.2f,%s,%s,%s,%s,%s,%.6f,%.6f,%.6f,%.6f\n", \
                x, y, hv, c1, c2, c3, x_log_s, y_log_s, x_asinh, y_asinh, x_logicle, y_logicle
        }
    }' > "$csv_path"

    echo "[OK] Generated 5000 rows"
}

# ============================================================================
# 4. Create project and upload data
# ============================================================================

create_project_and_upload() {
    local DATE_TAG
    DATE_TAG=$(date +%Y%m%d_%H%M%S)
    PROJECT_NAME="showcase_${DATE_TAG}"

    echo "[INFO] Creating project '${PROJECT_NAME}'..."
    local proj_output
    proj_output=$(tercenctl project create -n "$PROJECT_NAME" -f json 2>/dev/null)
    PROJECT_ID=$(echo "$proj_output" | json_field "id")
    if [[ -z "$PROJECT_ID" ]]; then
        echo "[ERROR] Failed to create project"; exit 1
    fi
    echo "[OK] Project: $PROJECT_ID"

    local csv_tmp
    csv_tmp=$(mktemp /tmp/plot_data_XXXXXX.csv)
    trap "rm -f '$csv_tmp'" EXIT

    generate_csv "$csv_tmp"

    echo "[INFO] Uploading CSV..."
    local upload_output
    upload_output=$(tercenctl data upload-csv -p "$PROJECT_ID" --filePath "$csv_tmp" -n "plot_data" -f json 2>/dev/null)
    SCHEMA_ID=$(echo "$upload_output" | json_field "id")
    if [[ -z "$SCHEMA_ID" ]]; then
        echo "[ERROR] Failed to upload CSV"; exit 1
    fi
    echo "[OK] Schema: $SCHEMA_ID"
}

# ============================================================================
# 5. Create workflow with table step
# ============================================================================

add_table_step() {
    local wf_id="$1" schema_id="$2"
    local ts_id="ts-$(uuidgen)"

    local patch
    patch=$(cat <<EOF
{
  "kind": "PatchRecords", "oI": "$wf_id",
  "rs": [{"kind": "PatchRecord", "p": "/steps",
    "recordType": {"kind": "PatchRecordListAdd",
      "value": [{"kind": "TypedObject", "value": {
        "kind": "TableStep", "id": "$ts_id", "groupId": "", "name": "data",
        "inputs": [],
        "outputs": [{"kind": "OutputPort", "id": "${ts_id}-o-0", "linkType": "relation", "name": "table"}],
        "rectangle": {"kind": "Rectangle",
          "extent": {"kind": "Point", "x": 100, "y": 55},
          "topLeft": {"kind": "Point", "x": 250, "y": 120}},
        "state": {"kind": "StepState", "taskId": "", "taskState": {"kind": "DoneState"}},
        "description": "",
        "model": {"kind": "TableStepModel",
          "relation": {"kind": "SimpleRelation", "id": "$schema_id", "index": 0},
          "filterSelector": ""}
      }}]}}]
}
EOF
    )

    tercenctl object patch -p "$patch" &>/dev/null || { echo "[ERROR] Failed to add TableStep"; exit 1; }
    echo "$ts_id"
}

create_workflow() {
    echo "[INFO] Creating workflow..."
    local wf_output
    wf_output=$(tercenctl workflow create -p "$PROJECT_ID" -n "showcase" -f json 2>/dev/null)
    WORKFLOW_ID=$(echo "$wf_output" | json_field "id")
    if [[ -z "$WORKFLOW_ID" ]]; then
        echo "[ERROR] Failed to create workflow"; exit 1
    fi
    echo "[OK] Workflow: $WORKFLOW_ID"

    TABLE_STEP_ID=$(add_table_step "$WORKFLOW_ID" "$SCHEMA_ID")
    echo "[OK] TableStep: $TABLE_STEP_ID"
}

# ============================================================================
# 6. Patching functions
# ============================================================================

patch_factor() {
    local wf_id="$1" step_id="$2" path="$3" factor_name="$4" factor_type="$5"

    local patch
    patch=$(cat <<EOF
{"kind": "PatchRecords", "oI": "$wf_id",
 "rs": [{"kind": "PatchRecord", "p": "/steps/@[id=$step_id]/$path",
   "recordType": {"kind": "PatchRecordSet",
     "value": {"kind": "TypedObject",
       "value": {"kind": "Factor", "name": "$factor_name", "type": "$factor_type"}}}}]}
EOF
    )
    tercenctl object patch -p "$patch" &>/dev/null
}

add_color_factor() {
    local wf_id="$1" step_id="$2" factor_name="$3" factor_type="$4"

    local patch
    patch=$(cat <<EOF
{"kind": "PatchRecords", "oI": "$wf_id",
 "rs": [{"kind": "PatchRecord",
   "p": "/steps/@[id=$step_id]/model/axis/xyAxis/@0/colors/factors",
   "recordType": {"kind": "PatchRecordListAdd",
     "value": [{"kind": "TypedObject",
       "value": {"kind": "Factor", "name": "$factor_name", "type": "$factor_type"}}]}}]}
EOF
    )
    tercenctl object patch -p "$patch" &>/dev/null
}

patch_ramp_palette() {
    local wf_id="$1" step_id="$2" palette_name="$3"

    local patch
    patch=$(cat <<EOF
{"kind": "PatchRecords", "oI": "$wf_id",
 "rs": [{"kind": "PatchRecord",
   "p": "/steps/@[id=$step_id]/model/axis/xyAxis/@0/colors/palette",
   "recordType": {"kind": "PatchRecordSet",
     "value": {"kind": "TypedObject",
       "value": {"kind": "RampPalette",
         "backcolor": -1,
         "isUserDefined": false,
         "properties": [{"kind": "PropertyValue", "name": "name", "value": "$palette_name"}],
         "doubleColorElements": [
           {"kind": "DoubleColorElement", "color": -16776961, "stringValue": "0"},
           {"kind": "DoubleColorElement", "color": -65536, "stringValue": "100"}
         ]}}}}]}
EOF
    )
    tercenctl object patch -p "$patch" &>/dev/null
}

patch_chart_type() {
    local wf_id="$1" step_id="$2" chart_kind="$3"

    local chart_value
    case "$chart_kind" in
        point) chart_value='{"kind": "ChartPoint", "pointSize": 4}' ;;
        line)  chart_value='{"kind": "ChartLine", "pointSize": 4}' ;;
        bar)   chart_value='{"kind": "ChartBar"}' ;;
        heatmap) chart_value='{"kind": "ChartHeatmap"}' ;;
        *) echo "[ERROR] Unknown chart kind: $chart_kind"; return 1 ;;
    esac

    local patch
    patch=$(cat <<EOF
{"kind": "PatchRecords", "oI": "$wf_id",
 "rs": [{"kind": "PatchRecord",
   "p": "/steps/@[id=$step_id]/model/axis/xyAxis/@0/chart",
   "recordType": {"kind": "PatchRecordSet",
     "value": {"kind": "TypedObject", "value": $chart_value}}}]}
EOF
    )
    tercenctl object patch -p "$patch" &>/dev/null
}

# ============================================================================
# 7. Create and configure a scenario step
# ============================================================================

# Usage: create_scenario_step NAME CHART_KIND Y_COL Y_TYPE [X_COL X_TYPE] [COLOR_COL COLOR_TYPE]
# For heatmap: create_heatmap_step NAME Y_COL Y_TYPE ROW_FACTOR COL_FACTOR
create_scenario_step() {
    local name="$1" chart_kind="$2"
    local y_col="$3" y_type="$4"
    local x_col="${5:-}" x_type="${6:-}"
    local color_col="${7:-}" color_type="${8:-}"

    echo -n "  $name: "

    # Add data step
    local ds_output
    ds_output=$(tercenctl step add -w "$WORKFLOW_ID" -t data -n "$name" \
        --inputStepId "$TABLE_STEP_ID" -f json 2>/dev/null)
    local step_id
    step_id=$(echo "$ds_output" | json_field "id")
    if [[ -z "$step_id" ]]; then
        echo "FAILED (step add)"
        return 1
    fi

    # Set Y axis
    patch_factor "$WORKFLOW_ID" "$step_id" "model/axis/xyAxis/@0/yAxis/graphicalFactor/factor" "$y_col" "$y_type"

    # Set X axis (if provided)
    if [[ -n "$x_col" ]]; then
        patch_factor "$WORKFLOW_ID" "$step_id" "model/axis/xyAxis/@0/xAxis/graphicalFactor/factor" "$x_col" "$x_type"
    fi

    # Set color (if provided)
    if [[ -n "$color_col" ]]; then
        add_color_factor "$WORKFLOW_ID" "$step_id" "$color_col" "$color_type"
    fi

    # Set chart type (default is point, so skip if point)
    if [[ "$chart_kind" != "point" ]]; then
        patch_chart_type "$WORKFLOW_ID" "$step_id" "$chart_kind"
    fi

    # Set operator
    tercenctl step set-operator -w "$WORKFLOW_ID" -s "$step_id" -o "$OPERATOR_ID" &>/dev/null

    SCENARIO_STEPS["$name"]="$step_id"
    echo "OK ($step_id)"
}

create_heatmap_step() {
    local name="$1"
    local y_col="$2" y_type="$3"
    local row_factor="$4" col_factor="$5"

    echo -n "  $name: "

    local ds_output
    ds_output=$(tercenctl step add -w "$WORKFLOW_ID" -t data -n "$name" \
        --inputStepId "$TABLE_STEP_ID" -f json 2>/dev/null)
    local step_id
    step_id=$(echo "$ds_output" | json_field "id")
    if [[ -z "$step_id" ]]; then
        echo "FAILED (step add)"
        return 1
    fi

    # Set Y axis
    patch_factor "$WORKFLOW_ID" "$step_id" "model/axis/xyAxis/@0/yAxis/graphicalFactor/factor" "$y_col" "$y_type"

    # Set row factor (on crosstab row table)
    patch_factor "$WORKFLOW_ID" "$step_id" "model/rowTable/graphicalFactors/@0/factor" "$row_factor" "string"

    # Set column factor (on crosstab column table)
    patch_factor "$WORKFLOW_ID" "$step_id" "model/columnTable/graphicalFactors/@0/factor" "$col_factor" "string"

    # Set chart type to heatmap
    patch_chart_type "$WORKFLOW_ID" "$step_id" "heatmap"

    # Set color factor (Y values) and palette for heatmap coloring
    add_color_factor "$WORKFLOW_ID" "$step_id" "$y_col" "$y_type"
    patch_ramp_palette "$WORKFLOW_ID" "$step_id" "Spectral"

    # Set operator
    tercenctl step set-operator -w "$WORKFLOW_ID" -s "$step_id" -o "$OPERATOR_ID" &>/dev/null

    SCENARIO_STEPS["$name"]="$step_id"
    echo "OK ($step_id)"
}

# ============================================================================
# 8. Prepare step (create CubeQueryTask)
# ============================================================================

build_binaries() {
    echo "[INFO] Building binaries..."
    cargo build --bin prepare --bin dev --profile dev-release \
        --manifest-path "${SCRIPT_DIR}/Cargo.toml" 2>&1 | tail -2
}

prepare_step() {
    local name="$1"
    local step_id="${SCENARIO_STEPS[$name]}"
    local token
    token=$(grpc_token)

    echo -n "  $name: "
    local output
    output=$(TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
        "$PREPARE_BIN" --workflow-id "$WORKFLOW_ID" --step-id "$step_id" 2>&1)

    if echo "$output" | grep -q "Done!"; then
        echo "OK"
    elif echo "$output" | grep -q "Nothing to do"; then
        echo "OK (cached)"
    else
        echo "FAILED"
        echo "$output" | tail -3
        return 1
    fi
}

# ============================================================================
# 9. Render a single plot
# ============================================================================

render_plot() {
    local name="$1" backend="$2" theme="$3" flags="$4"
    local extra_json="${5:-}"   # Optional extra JSON properties (comma-separated key:value pairs)
    local step_id="${SCENARIO_STEPS[$name]}"
    local output_path="${OUTPUT_DIR}/${name}_${backend}_${theme}_${flags}.png"

    # Decode flags: T=text, A=axis, M=major grid, G=minor grid (1=disabled)
    local text_dis="${flags:0:1}" axis_dis="${flags:1:1}" gmaj_dis="${flags:2:1}" gmin_dis="${flags:3:1}"
    local text_val="false" axis_val="false" gmaj_val="false" gmin_val="false"
    [[ "$text_dis" == "1" ]] && text_val="true"
    [[ "$axis_dis" == "1" ]] && axis_val="true"
    [[ "$gmaj_dis" == "1" ]] && gmaj_val="true"
    [[ "$gmin_dis" == "1" ]] && gmin_val="true"

    # Build extra properties block
    local extra_block=""
    if [[ -n "$extra_json" ]]; then
        extra_block=",
    $extra_json"
    fi

    local token
    token=$(grpc_token)

    cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "$backend",
    "theme": "$theme",
    "output.format": "png",
    "plot.width": "800",
    "plot.height": "600",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "1",
    "text.disable": "$text_val",
    "axis.lines.disable": "$axis_val",
    "grid.major.disable": "$gmaj_val",
    "grid.minor.disable": "$gmin_val"${extra_block}
}
EOF

    (
        cd "$SCRIPT_DIR"
        TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
            WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
            "$DEV_BIN" >/dev/null 2>&1
    )

    if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
        mv "${SCRIPT_DIR}/plot.png" "$output_path"
        return 0
    else
        echo "[WARN] No output for ${name}_${backend}_${theme}_${flags}"
        return 1
    fi
}

render_scenario() {
    local name="$1"
    local count=0 total=$(( ${#BACKENDS[@]} * ${#THEMES[@]} ))

    for backend in "${BACKENDS[@]}"; do
        for theme in "${THEMES[@]}"; do
            count=$((count + 1))
            printf "\r  $name: %d/%d" "$count" "$total"
            render_plot "$name" "$backend" "$theme" "0000" || true
        done
    done
    printf "\r  $name: %d/%d OK\n" "$total" "$total"
}

render_heatmap_scenario() {
    local name="$1"
    local step_id="${SCENARIO_STEPS[$name]}"
    local count=0 total=$(( ${#BACKENDS[@]} * ${#THEMES[@]} * ${#HEATMAP_PALETTES[@]} ))

    for palette in "${HEATMAP_PALETTES[@]}"; do
        # Re-patch the palette on the step before rendering this batch
        patch_ramp_palette "$WORKFLOW_ID" "$step_id" "$palette"

        for backend in "${BACKENDS[@]}"; do
            for theme in "${THEMES[@]}"; do
                count=$((count + 1))
                printf "\r  $name: %d/%d" "$count" "$total"

                local output_path="${OUTPUT_DIR}/${name}_${backend}_${theme}_${palette}_0000.png"
                local token
                token=$(grpc_token)

                cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "$backend",
    "theme": "$theme",
    "output.format": "png",
    "plot.width": "800",
    "plot.height": "600",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "1"
}
EOF

                (
                    cd "$SCRIPT_DIR"
                    TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
                        WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
                        "$DEV_BIN" >/dev/null 2>&1
                )

                if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
                    mv "${SCRIPT_DIR}/plot.png" "$output_path"
                else
                    echo "[WARN] No output for ${name}_${backend}_${theme}_${palette}"
                fi
            done
        done
    done
    printf "\r  $name: %d/%d OK\n" "$total" "$total"
}

# Extra properties for toggle renders: axis labels + plot title
TOGGLE_EXTRA='"plot.title": "Plot Title", "axis.x.label": "X Label", "axis.y.label": "Y Label"'

# Render toggle combinations for a single scenario (bw theme, cpu backend)
render_toggle_combos() {
    local name="$1" backend="cpu" theme="bw"
    local step_id="${SCENARIO_STEPS[$name]}"
    local count=0 total=${#FLAGS_COMBOS[@]}

    for flags in "${FLAGS_COMBOS[@]}"; do
        count=$((count + 1))
        printf "\r  toggle_$name: %d/%d" "$count" "$total"
        render_plot "$name" "$backend" "$theme" "$flags" "$TOGGLE_EXTRA" || true
    done
    printf "\r  toggle_$name: %d/%d OK\n" "$total" "$total"
}

render_faceted() {
    local name="$1"
    local width="${2:-800}" height="${3:-600}"
    local step_id="${SCENARIO_STEPS[$name]}"
    local output_path="${OUTPUT_DIR}/${name}_cpu_bw_0000.png"

    local token
    token=$(grpc_token)

    cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "cpu",
    "theme": "bw",
    "output.format": "png",
    "plot.width": "$width",
    "plot.height": "$height",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "1",
    "plot.title": "Plot Title",
    "axis.x.label": "X Label",
    "axis.y.label": "Y Label"
}
EOF

    (
        cd "$SCRIPT_DIR"
        TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
            WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
            "$DEV_BIN" >/dev/null 2>&1
    )

    if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
        mv "${SCRIPT_DIR}/plot.png" "$output_path"
        echo "  $name: OK"
    else
        echo "  $name: FAILED"
    fi
}

render_transform_combos() {
    # Renders transform showcase: "none" uses scatter_cat (same clean bands as other sections),
    # others use scatter_tf_<transform> (pre-transformed data) + transform override.
    local count=0 total=${#TRANSFORMS[@]}

    for tf in "${TRANSFORMS[@]}"; do
        count=$((count + 1))
        printf "\r  transforms: %d/%d" "$count" "$total"

        local step_name tf_block output_path
        if [[ "$tf" == "none" ]]; then
            step_name="scatter_cat"
            tf_block=""
        else
            step_name="scatter_tf_${tf}"
            tf_block="\"axis.y.transform\": \"$tf\",
    \"axis.x.transform\": \"$tf\","
        fi

        local step_id="${SCENARIO_STEPS[$step_name]}"
        output_path="${OUTPUT_DIR}/scatter_cat_cpu_bw_tf_${tf}.png"

        local token
        token=$(grpc_token)

        cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "cpu",
    "theme": "bw",
    "output.format": "png",
    "plot.width": "800",
    "plot.height": "600",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "1",
    ${tf_block}
    "plot.title": "Plot Title",
    "axis.x.label": "X Label",
    "axis.y.label": "Y Label"
}
EOF

        (
            cd "$SCRIPT_DIR"
            TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
                WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
                "$DEV_BIN" >/dev/null 2>&1
        )

        if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
            mv "${SCRIPT_DIR}/plot.png" "$output_path"
        else
            echo "[WARN] No output for transform ${tf}"
        fi
    done
    printf "\r  transforms: %d/%d OK\n" "$total" "$total"
}

render_marker_shapes() {
    # Renders scatter_cat with each pch shape (0-25), bw theme, cpu backend
    local count=0 total=26

    for pch in $(seq 0 25); do
        count=$((count + 1))
        printf "\r  markers: %d/%d" "$count" "$total"

        local step_id="${SCENARIO_STEPS[scatter_cat]}"
        local output_path="${OUTPUT_DIR}/scatter_cat_cpu_bw_pch_${pch}.png"

        local token
        token=$(grpc_token)

        cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "cpu",
    "theme": "bw",
    "output.format": "png",
    "plot.width": "800",
    "plot.height": "600",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "1",
    "point.shapes": "$pch",
    "plot.title": "Plot Title",
    "axis.x.label": "X Label",
    "axis.y.label": "Y Label"
}
EOF

        (
            cd "$SCRIPT_DIR"
            TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
                WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
                "$DEV_BIN" >/dev/null 2>&1
        )

        if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
            mv "${SCRIPT_DIR}/plot.png" "$output_path"
        else
            echo "[WARN] No output for pch ${pch}"
        fi
    done
    printf "\r  markers: %d/%d OK\n" "$total" "$total"
}

OPACITIES=(0.25 0.5 0.75 1)

render_opacity_combos() {
    # Renders scatter_cat with different opacity levels, bw theme, cpu backend
    local count=0 total=${#OPACITIES[@]}

    for op in "${OPACITIES[@]}"; do
        count=$((count + 1))
        printf "\r  opacity: %d/%d" "$count" "$total"

        local step_id="${SCENARIO_STEPS[scatter_cat]}"
        local output_path="${OUTPUT_DIR}/scatter_cat_cpu_bw_opacity_${op}.png"

        local token
        token=$(grpc_token)

        cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "cpu",
    "theme": "bw",
    "output.format": "png",
    "plot.width": "800",
    "plot.height": "600",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "$op",
    "plot.title": "Plot Title",
    "axis.x.label": "X Label",
    "axis.y.label": "Y Label"
}
EOF

        (
            cd "$SCRIPT_DIR"
            TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
                WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
                "$DEV_BIN" >/dev/null 2>&1
        )

        if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
            mv "${SCRIPT_DIR}/plot.png" "$output_path"
        else
            echo "[WARN] No output for opacity ${op}"
        fi
    done
    printf "\r  opacity: %d/%d OK\n" "$total" "$total"
}

render_heatmap_toggle_combos() {
    local name="$1" backend="cpu" theme="bw" palette="Jet"
    local step_id="${SCENARIO_STEPS[$name]}"
    local count=0 total=${#FLAGS_COMBOS[@]}

    # Patch palette to Jet for toggle section
    patch_ramp_palette "$WORKFLOW_ID" "$step_id" "$palette"

    for flags in "${FLAGS_COMBOS[@]}"; do
        count=$((count + 1))
        printf "\r  toggle_$name: %d/%d" "$count" "$total"

        local output_path="${OUTPUT_DIR}/${name}_${backend}_${theme}_${palette}_${flags}.png"

        # Decode flags
        local text_dis="${flags:0:1}" axis_dis="${flags:1:1}" gmaj_dis="${flags:2:1}" gmin_dis="${flags:3:1}"
        local text_val="false" axis_val="false" gmaj_val="false" gmin_val="false"
        [[ "$text_dis" == "1" ]] && text_val="true"
        [[ "$axis_dis" == "1" ]] && axis_val="true"
        [[ "$gmaj_dis" == "1" ]] && gmaj_val="true"
        [[ "$gmin_dis" == "1" ]] && gmin_val="true"

        local token
        token=$(grpc_token)

        cat > "${SCRIPT_DIR}/operator_config.json" <<EOF
{
    "backend": "$backend",
    "theme": "$theme",
    "output.format": "png",
    "plot.width": "800",
    "plot.height": "600",
    "png.compression": "fast",
    "legend.position": "right",
    "point.size.multiplier": "1",
    "opacity": "1",
    "text.disable": "$text_val",
    "axis.lines.disable": "$axis_val",
    "grid.major.disable": "$gmaj_val",
    "grid.minor.disable": "$gmin_val",
    "plot.title": "Plot Title",
    "axis.x.label": "X Label",
    "axis.y.label": "Y Label"
}
EOF

        (
            cd "$SCRIPT_DIR"
            TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
                WORKFLOW_ID="$WORKFLOW_ID" STEP_ID="$step_id" \
                "$DEV_BIN" >/dev/null 2>&1
        )

        if [[ -f "${SCRIPT_DIR}/plot.png" ]]; then
            mv "${SCRIPT_DIR}/plot.png" "$output_path"
        else
            echo "[WARN] No output for toggle ${name}_${flags}"
        fi
    done
    printf "\r  toggle_$name: %d/%d OK\n" "$total" "$total"
}

# ============================================================================
# 10. Cleanup
# ============================================================================

delete_project() {
    local project_id="$1"
    local token
    token=$(grpc_token)
    TERCEN_URI="$TERCEN_GRPC_URI" TERCEN_TOKEN="$token" \
        "$PREPARE_BIN" --delete-project "$project_id" 2>&1
}

# ============================================================================
# 11. Generate showcase.html
# ============================================================================

generate_html() {
    # Build the scenarios JSON for JavaScript
    # Format: { sectionName: { label, variants: [{name, label}] } }

    cat > "$SHOWCASE_HTML" <<'HTMLEOF'
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>GGRS Plot Operator — Showcase</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  html { scroll-behavior: smooth; scroll-padding-top: 4rem; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: #f5f5f5; color: #333;
    max-width: 1000px; margin: 0 auto; padding: 0 2rem 2rem;
  }

  /* --- Sticky nav --- */
  nav {
    position: sticky; top: 0; z-index: 100;
    background: #fff; border-bottom: 1px solid #e0e0e0;
    margin: 0 -2rem; padding: 0 2rem;
    box-shadow: 0 1px 4px rgba(0,0,0,0.06);
  }
  nav .nav-inner {
    display: flex; gap: 0; overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    scrollbar-width: none;
  }
  nav .nav-inner::-webkit-scrollbar { display: none; }
  nav .nav-group {
    display: flex; gap: 0; align-items: stretch;
    border-right: 1px solid #e8e8e8;
  }
  nav .nav-group:last-child { border-right: none; }
  nav .nav-group-label {
    font-size: 0.65rem; text-transform: uppercase; letter-spacing: 0.05em;
    color: #999; padding: 0.25rem 0.6rem 0; white-space: nowrap;
    align-self: flex-start;
  }
  nav a {
    display: flex; align-items: center;
    padding: 0.6rem 0.7rem; font-size: 0.8rem; color: #555;
    text-decoration: none; white-space: nowrap;
    border-bottom: 2px solid transparent;
    transition: color 0.15s, border-color 0.15s;
  }
  nav a:hover { color: #111; }
  nav a.active { color: #2563eb; border-bottom-color: #2563eb; font-weight: 600; }

  /* --- Header --- */
  .header { padding: 2rem 0 0.5rem; }
  .header h1 { font-size: 1.8rem; margin-bottom: 0.3rem; }
  .header .subtitle { color: #666; font-size: 0.95rem; margin-bottom: 0; }
  .header .build-info {
    font-size: 0.8rem; color: #999; margin-top: 0.3rem;
  }
  .header .build-info code {
    background: #f0f0f0; padding: 0.1rem 0.4rem; border-radius: 3px;
    font-family: "SF Mono", "Fira Code", monospace; font-size: 0.78rem;
  }

  /* --- Intro --- */
  .intro {
    background: #fff; border-radius: 8px; padding: 1.5rem;
    margin: 1.5rem 0; box-shadow: 0 1px 3px rgba(0,0,0,0.1);
    line-height: 1.6; font-size: 0.92rem; color: #444;
  }
  .intro h2 { font-size: 1.1rem; margin-bottom: 0.8rem; color: #333; }
  .intro ul { margin: 0.5rem 0 0.5rem 1.5rem; }
  .intro li { margin-bottom: 0.3rem; }
  .intro strong { color: #333; }
  .intro .tech-note { font-size: 0.82rem; color: #888; margin-top: 0.8rem; }

  /* --- Category headers --- */
  .category-header {
    font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.08em;
    color: #999; font-weight: 600; margin: 2rem 0 0.8rem;
    padding-bottom: 0.3rem; border-bottom: 1px solid #e0e0e0;
  }

  /* --- Section cards --- */
  section { background: #fff; border-radius: 8px; padding: 1.5rem;
    margin-bottom: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
  section h2 { font-size: 1.3rem; margin-bottom: 1rem; }
  .controls { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem; align-items: flex-end; }
  .control-group { display: flex; flex-direction: column; gap: 0.2rem; }
  .control-group label { font-size: 0.8rem; color: #666; font-weight: 500; }
  select {
    padding: 0.4rem 0.8rem; border: 1px solid #ccc; border-radius: 4px;
    font-size: 0.9rem; background: #fff; cursor: pointer;
  }
  select:hover { border-color: #888; }
  .toggles { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem; }
  .toggle-group { display: flex; align-items: center; gap: 0.3rem; }
  .toggle-group input[type="checkbox"] { cursor: pointer; }
  .toggle-group label { font-size: 0.85rem; color: #444; cursor: pointer; user-select: none; }
  .image-container {
    background: #fafafa; border: 1px solid #eee; border-radius: 4px;
    display: flex; justify-content: center; align-items: center;
    min-height: 300px; padding: 0.5rem;
  }
  .image-container img {
    max-width: 100%; height: auto; border-radius: 2px;
  }
  .image-container .error {
    color: #999; font-style: italic;
  }
  .filename { font-size: 0.75rem; color: #999; margin-top: 0.5rem; text-align: center; }
  .section-desc { color: #666; font-size: 0.9rem; margin-bottom: 1rem; }
</style>
</head>
<body>

<nav>
  <div class="nav-inner">
    <div class="nav-group">
      <span class="nav-group-label">Charts</span>
      <a href="#sec-scatter">Scatter</a>
      <a href="#sec-line">Line</a>
      <a href="#sec-heatmap">Heatmap</a>
    </div>
    <div class="nav-group">
      <span class="nav-group-label">Layout</span>
      <a href="#sec-facets">Facets</a>
    </div>
    <div class="nav-group">
      <span class="nav-group-label">Properties</span>
      <a href="#sec-markers">Markers</a>
      <a href="#sec-opacity">Opacity</a>
      <a href="#sec-transforms">Transforms</a>
    </div>
    <div class="nav-group">
      <span class="nav-group-label">Visibility</span>
      <a href="#sec-toggles">Toggles</a>
    </div>
  </div>
</nav>

<div class="header">
  <h1>GGRS Plot Operator — Showcase</h1>
  <p class="subtitle">Interactive visual reference for the Tercen GGRS plotting operator</p>
HTMLEOF

    # Stamp commit SHA and generation date if available
    local commit_sha="${SHOWCASE_COMMIT:-$(git -C "$SCRIPT_DIR" rev-parse HEAD 2>/dev/null || echo 'unknown')}"
    # Show short form (first 8 chars) for display
    local commit_short="${commit_sha:0:8}"
    local gen_date
    gen_date=$(date -u +"%Y-%m-%d %H:%M UTC")
    cat >> "$SHOWCASE_HTML" <<HTMLEOF
  <p class="build-info">Generated on ${gen_date} from commit <code>${commit_short}</code></p>
</div>
HTMLEOF

    cat >> "$SHOWCASE_HTML" <<'HTMLEOF'

<div class="intro">
  <h2>About this showcase</h2>
  <p>
    This page demonstrates the rendering capabilities of the <strong>GGRS Plot Operator</strong>,
    a Rust-based Tercen operator that generates publication-quality plots from crosstab data.
    All images were generated from synthetic test data (5 000 rows, 4 categorical groups) and
    rendered against a live Tercen instance.
  </p>
  <ul>
    <li><strong>Chart types</strong> — Scatter, line, and heatmap with optional categorical color</li>
    <li><strong>9 themes</strong> — gray, bw, light, dark, minimal, classic, linedraw, void, publish</li>
    <li><strong>2 backends</strong> — CPU (Cairo) and GPU (WebGPU/Vulkan)</li>
    <li><strong>Faceting</strong> — Row, column, or both, using ggplot2-style facet grids</li>
    <li><strong>Visual properties</strong> — 26 marker shapes (pch 0-25), opacity control, axis transforms</li>
    <li><strong>Element toggles</strong> — Independent control of text, axis lines, major/minor grid</li>
  </ul>
  <p class="tech-note">
    Use the dropdowns and controls in each section to interactively compare options.
    Filename shown below each plot for reference.
  </p>
</div>

<div class="category-header" id="cat-charts">Chart Types</div>

<section id="sec-scatter">
  <h2>Scatter Plots</h2>
  <div class="controls">
    <div class="control-group">
      <label>Color</label>
      <select data-section="scatter" data-dim="variant">
        <option value="scatter_nocolor">No color</option>
        <option value="scatter_cat">Categorical (CAT1)</option>
      </select>
    </div>
    <div class="control-group">
      <label>Theme</label>
      <select data-section="scatter" data-dim="theme">
        <option value="gray">gray</option>
        <option value="bw">bw</option>
        <option value="light">light</option>
        <option value="dark">dark</option>
        <option value="minimal">minimal</option>
        <option value="classic">classic</option>
        <option value="linedraw">linedraw</option>
        <option value="void">void</option>
        <option value="publish">publish</option>
      </select>
    </div>
    <div class="control-group">
      <label>Backend</label>
      <select data-section="scatter" data-dim="backend">
        <option value="cpu">CPU (Cairo)</option>
        <option value="gpu">GPU (WebGPU)</option>
      </select>
    </div>
  </div>
  <div class="image-container">
    <img id="img-scatter" src="showcase_output/scatter_nocolor_cpu_gray_0000.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Scatter plot">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-scatter">scatter_nocolor_cpu_gray_0000.png</div>
</section>

<section id="sec-line">
  <h2>Line Plots</h2>
  <div class="controls">
    <div class="control-group">
      <label>Color</label>
      <select data-section="line" data-dim="variant">
        <option value="line_nocolor">No color</option>
        <option value="line_cat">Categorical (CAT1)</option>
      </select>
    </div>
    <div class="control-group">
      <label>Theme</label>
      <select data-section="line" data-dim="theme">
        <option value="gray">gray</option>
        <option value="bw">bw</option>
        <option value="light">light</option>
        <option value="dark">dark</option>
        <option value="minimal">minimal</option>
        <option value="classic">classic</option>
        <option value="linedraw">linedraw</option>
        <option value="void">void</option>
        <option value="publish">publish</option>
      </select>
    </div>
    <div class="control-group">
      <label>Backend</label>
      <select data-section="line" data-dim="backend">
        <option value="cpu">CPU (Cairo)</option>
        <option value="gpu">GPU (WebGPU)</option>
      </select>
    </div>
  </div>
  <div class="image-container">
    <img id="img-line" src="showcase_output/line_nocolor_cpu_gray_0000.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Line plot">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-line">line_nocolor_cpu_gray_0000.png</div>
</section>

<section id="sec-heatmap">
  <h2>Heatmap</h2>
  <div class="controls">
    <div class="control-group">
      <label>Palette</label>
      <select data-section="heatmap" data-dim="palette">
        <option value="Spectral">Spectral</option>
        <option value="Jet">Jet</option>
        <option value="Viridis">Viridis</option>
        <option value="Hot">Hot</option>
        <option value="Cool">Cool</option>
        <option value="RdBu">RdBu</option>
        <option value="YlGnBu">YlGnBu</option>
      </select>
    </div>
    <div class="control-group">
      <label>Theme</label>
      <select data-section="heatmap" data-dim="theme">
        <option value="gray">gray</option>
        <option value="bw">bw</option>
        <option value="light">light</option>
        <option value="dark">dark</option>
        <option value="minimal">minimal</option>
        <option value="classic">classic</option>
        <option value="linedraw">linedraw</option>
        <option value="void">void</option>
        <option value="publish">publish</option>
      </select>
    </div>
    <div class="control-group">
      <label>Backend</label>
      <select data-section="heatmap" data-dim="backend">
        <option value="cpu">CPU (Cairo)</option>
        <option value="gpu">GPU (WebGPU)</option>
      </select>
    </div>
  </div>
  <div class="image-container">
    <img id="img-heatmap" src="showcase_output/heatmap_cpu_gray_Spectral_0000.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Heatmap">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-heatmap">heatmap_cpu_gray_Spectral_0000.png</div>
</section>

<div class="category-header" id="cat-layout">Layout</div>

<section id="sec-facets">
  <h2>Faceted Plots</h2>
  <p class="section-desc">
    Faceting with categorical color (CAT1). Row facets use CAT2 (4 levels),
    column facets use CAT2 or CAT3. Rendered with <strong>bw</strong> theme,
    <strong>CPU</strong> backend, axis labels and plot title.
  </p>
  <div class="controls">
    <div class="control-group">
      <label>Chart type</label>
      <select id="facet-chart-type">
        <option value="scatter">Scatter</option>
        <option value="line">Line</option>
      </select>
    </div>
    <div class="control-group">
      <label>Faceting</label>
      <select id="facet-dim">
        <option value="row">Row facets (~ facet_grid(CAT2 ~ .))</option>
        <option value="col">Column facets (~ facet_grid(. ~ CAT2))</option>
        <option value="both">Both (~ facet_grid(CAT2 ~ CAT3))</option>
      </select>
    </div>
  </div>
  <div class="image-container">
    <img id="img-facets" src="showcase_output/scatter_facet_row_cpu_bw_0000.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Faceted plot">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-facets">scatter_facet_row_cpu_bw_0000.png</div>
</section>

<div class="category-header" id="cat-properties">Visual Properties</div>

<section id="sec-markers">
  <h2>Marker Shapes</h2>
  <p class="section-desc">
    All 26 ggplot2 point shapes (pch 0–25). Rendered with categorical color (CAT1),
    <strong>bw</strong> theme, <strong>CPU</strong> backend, axis labels and plot title.
    Shapes 0–14 are hollow (outline only), 15–20 are filled, 21–25 are filled with black border.
  </p>
  <div class="controls">
    <div class="control-group">
      <label>Shape (pch)</label>
      <select id="marker-select">
        <option value="0">0 — hollow square</option>
        <option value="1">1 — hollow circle</option>
        <option value="2">2 — hollow triangle up</option>
        <option value="3">3 — plus</option>
        <option value="4">4 — cross (X)</option>
        <option value="5">5 — hollow diamond</option>
        <option value="6">6 — hollow triangle down</option>
        <option value="7">7 — square cross</option>
        <option value="8">8 — asterisk</option>
        <option value="9">9 — diamond plus</option>
        <option value="10">10 — circle plus</option>
        <option value="11">11 — star (two triangles)</option>
        <option value="12">12 — square plus</option>
        <option value="13">13 — circle cross</option>
        <option value="14">14 — square triangle</option>
        <option value="15">15 — filled square</option>
        <option value="16">16 — filled circle</option>
        <option value="17">17 — filled triangle up</option>
        <option value="18">18 — filled diamond</option>
        <option value="19" selected>19 — solid circle (default)</option>
        <option value="20">20 — bullet (small circle)</option>
        <option value="21">21 — circle with border</option>
        <option value="22">22 — square with border</option>
        <option value="23">23 — diamond with border</option>
        <option value="24">24 — triangle up with border</option>
        <option value="25">25 — triangle down with border</option>
      </select>
    </div>
  </div>
  <div class="image-container">
    <img id="img-markers" src="showcase_output/scatter_cat_cpu_bw_pch_19.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Marker shape">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-markers">scatter_cat_cpu_bw_pch_19.png</div>
</section>

<section id="sec-opacity">
  <h2>Opacity</h2>
  <p class="section-desc">
    Global opacity applied to all data geoms (points, tiles, bars, lines).
    Non-data elements (axes, labels, grid) stay fully opaque.
    Rendered with categorical color (CAT1), <strong>bw</strong> theme,
    <strong>CPU</strong> backend, axis labels and plot title.
  </p>
  <div class="controls" style="align-items:center;">
    <div class="control-group">
      <label>Opacity</label>
      <div style="display:flex;align-items:center;gap:0.8rem;">
        <input type="range" id="opacity-slider" min="0" max="3" step="1" value="3"
               style="width:180px;cursor:pointer;">
        <span id="opacity-value" style="font-size:0.95rem;font-weight:500;min-width:2.5rem;">1</span>
      </div>
    </div>
  </div>
  <div class="image-container">
    <img id="img-opacity" src="showcase_output/scatter_cat_cpu_bw_opacity_1.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Opacity">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-opacity">scatter_cat_cpu_bw_opacity_1.png</div>
</section>

<section id="sec-transforms">
  <h2>Axis Transforms</h2>
  <p class="section-desc">
    Scatter plot with categorical color (CAT1), applying the same transform to both axes.
    Currently only <strong>log</strong>, <strong>asinh</strong>, and <strong>logicle</strong>
    are selectable in the Tercen UI. Rendered with <strong>bw</strong> theme,
    <strong>CPU</strong> backend, axis labels and plot title.
  </p>
  <div class="controls">
    <div class="control-group">
      <label>Transform</label>
      <select id="tf-select">
        <option value="none">None (identity)</option>
        <option value="log">Log (natural log)</option>
        <option value="asinh">Asinh (arcsinh)</option>
        <option value="logicle">Logicle</option>
      </select>
    </div>
  </div>
  <div class="image-container">
    <img id="img-transforms" src="showcase_output/scatter_cat_cpu_bw_tf_none.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Axis transform">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-transforms">scatter_cat_cpu_bw_tf_none.png</div>
</section>

<div class="category-header" id="cat-visibility">Element Visibility</div>

<section id="sec-toggles">
  <h2>Element Toggles</h2>
  <p class="section-desc">
    Toggle plot elements on/off. All rendered with <strong>bw</strong> theme,
    <strong>CPU</strong> backend, categorical color, axis labels, and plot title.
  </p>
  <div class="controls">
    <div class="control-group">
      <label>Chart type</label>
      <select id="tog-chart-type">
        <option value="scatter">Scatter</option>
        <option value="line">Line</option>
        <option value="heatmap">Heatmap</option>
      </select>
    </div>
  </div>
  <div class="toggles" style="margin-bottom:1rem;">
    <div class="toggle-group">
      <input type="checkbox" id="tog-text" checked>
      <label for="tog-text">Text labels</label>
    </div>
    <div class="toggle-group">
      <input type="checkbox" id="tog-axis" checked>
      <label for="tog-axis">Axis lines</label>
    </div>
    <div class="toggle-group">
      <input type="checkbox" id="tog-gmaj" checked>
      <label for="tog-gmaj">Major grid</label>
    </div>
    <div class="toggle-group">
      <input type="checkbox" id="tog-gmin" checked>
      <label for="tog-gmin">Minor grid</label>
    </div>
  </div>
  <div class="image-container">
    <img id="img-toggles" src="showcase_output/scatter_cat_cpu_bw_0000.png"
         onerror="this.style.display='none';this.nextElementSibling.style.display='block'"
         alt="Element toggles">
    <span class="error" style="display:none">Image not available</span>
  </div>
  <div class="filename" id="fn-toggles">scatter_cat_cpu_bw_0000.png</div>
</section>

<script>
/* --- Main sections: theme/backend/variant selectors, always _0000 flags --- */
function updateMainImage(section) {
  const selects = document.querySelectorAll(`select[data-section="${section}"]`);
  const vals = {};
  selects.forEach(s => vals[s.dataset.dim] = s.value);

  const variant = vals.variant || section;
  const backend = vals.backend || 'cpu';
  const theme = vals.theme || 'gray';

  let filename;
  if (section === 'heatmap') {
    const palette = vals.palette || 'Spectral';
    filename = `${variant}_${backend}_${theme}_${palette}_0000.png`;
  } else {
    filename = `${variant}_${backend}_${theme}_0000.png`;
  }

  const img = document.getElementById(`img-${section}`);
  const fn = document.getElementById(`fn-${section}`);
  const err = img.nextElementSibling;
  img.style.display = '';
  err.style.display = 'none';
  img.src = `showcase_output/${filename}`;
  fn.textContent = filename;
}

/* --- Transforms section: transform selector --- */
function updateTransformImage() {
  const tf = document.getElementById('tf-select').value;
  const filename = `scatter_cat_cpu_bw_tf_${tf}.png`;

  const img = document.getElementById('img-transforms');
  const fn = document.getElementById('fn-transforms');
  img.style.display = ''; img.nextElementSibling.style.display = 'none';
  img.src = `showcase_output/${filename}`;
  fn.textContent = filename;
}

document.getElementById('tf-select').addEventListener('change', updateTransformImage);

/* --- Marker shapes section: pch selector --- */
function updateMarkerImage() {
  const pch = document.getElementById('marker-select').value;
  const filename = `scatter_cat_cpu_bw_pch_${pch}.png`;

  const img = document.getElementById('img-markers');
  const fn = document.getElementById('fn-markers');
  img.style.display = ''; img.nextElementSibling.style.display = 'none';
  img.src = `showcase_output/${filename}`;
  fn.textContent = filename;
}

document.getElementById('marker-select').addEventListener('change', updateMarkerImage);

/* --- Opacity section: range slider --- */
const opacitySteps = ['0.25', '0.5', '0.75', '1'];
function updateOpacityImage() {
  const idx = document.getElementById('opacity-slider').value;
  const op = opacitySteps[idx];
  document.getElementById('opacity-value').textContent = op;
  const filename = `scatter_cat_cpu_bw_opacity_${op}.png`;

  const img = document.getElementById('img-opacity');
  const fn = document.getElementById('fn-opacity');
  img.style.display = ''; img.nextElementSibling.style.display = 'none';
  img.src = `showcase_output/${filename}`;
  fn.textContent = filename;
}

document.getElementById('opacity-slider').addEventListener('input', updateOpacityImage);

/* --- Facets section: chart type + faceting dimension --- */
function updateFacetImage() {
  const chartType = document.getElementById('facet-chart-type').value;
  const facetDim = document.getElementById('facet-dim').value;
  const name = `${chartType}_facet_${facetDim}`;
  const filename = `${name}_cpu_bw_0000.png`;

  const img = document.getElementById('img-facets');
  const fn = document.getElementById('fn-facets');
  img.style.display = ''; img.nextElementSibling.style.display = 'none';
  img.src = `showcase_output/${filename}`;
  fn.textContent = filename;
}

document.getElementById('facet-chart-type').addEventListener('change', updateFacetImage);
document.getElementById('facet-dim').addEventListener('change', updateFacetImage);

/* --- Toggle section: checkboxes + chart type selector --- */
function updateToggleImage() {
  const chartType = document.getElementById('tog-chart-type').value;
  const flagOrder = ['text', 'axis', 'gmaj', 'gmin'];
  let flags = '';
  flagOrder.forEach(f => {
    const cb = document.getElementById(`tog-${f}`);
    flags += (cb && cb.checked) ? '0' : '1';
  });

  let filename;
  if (chartType === 'heatmap') {
    filename = `heatmap_cpu_bw_Jet_${flags}.png`;
  } else if (chartType === 'line') {
    filename = `line_cat_cpu_bw_${flags}.png`;
  } else {
    filename = `scatter_cat_cpu_bw_${flags}.png`;
  }

  const img = document.getElementById('img-toggles');
  const fn = document.getElementById('fn-toggles');
  img.style.display = ''; img.nextElementSibling.style.display = 'none';
  img.src = `showcase_output/${filename}`;
  fn.textContent = filename;
}

/* Wire up main section selectors */
document.querySelectorAll('select[data-section]').forEach(sel => {
  sel.addEventListener('change', () => updateMainImage(sel.dataset.section));
});

/* Wire up toggle controls */
document.getElementById('tog-chart-type').addEventListener('change', updateToggleImage);
document.querySelectorAll('#sec-toggles input[type="checkbox"]').forEach(cb => {
  cb.addEventListener('change', updateToggleImage);
});

/* --- Scroll spy: highlight active nav link --- */
(function() {
  const navLinks = document.querySelectorAll('nav a[href^="#"]');
  const sections = Array.from(navLinks).map(a => ({
    link: a,
    target: document.querySelector(a.getAttribute('href'))
  })).filter(s => s.target);

  function updateActive() {
    const scrollY = window.scrollY + 80;
    let current = sections[0];
    for (const s of sections) {
      if (s.target.offsetTop <= scrollY) current = s;
    }
    navLinks.forEach(a => a.classList.remove('active'));
    if (current) current.link.classList.add('active');
  }

  window.addEventListener('scroll', updateActive, { passive: true });
  updateActive();
})();
</script>

</body>
</html>
HTMLEOF

    echo "[OK] Generated $SHOWCASE_HTML"
}

# ============================================================================
# Main
# ============================================================================

echo "=== GGRS Plot Operator — Showcase Generator ==="
echo ""

check_tercenctl
ensure_context
build_binaries
create_project_and_upload
create_workflow

# Clean and create output directory
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

# --- Create all scenario steps ---
echo ""
echo "=== Creating scenario steps ==="

# Scatter variants
create_scenario_step "scatter_nocolor" "point" "y" "double" "x" "double"
create_scenario_step "scatter_cat"     "point" "y" "double" "x" "double" "CAT1" "string"

# Line variants
create_scenario_step "line_nocolor" "line" "y" "double" "x" "double"
create_scenario_step "line_cat"     "line" "y" "double" "x" "double" "CAT1" "string"

# Heatmap
create_heatmap_step "heatmap" "heatval" "double" "CAT1" "CAT2"

# Faceted scatter variants (CAT1 color, faceted by CAT2/CAT3)
create_scenario_step "scatter_facet_row"  "point" "y" "double" "x" "double" "CAT1" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[scatter_facet_row]}" \
    "model/rowTable/graphicalFactors/@0/factor" "CAT2" "string"

create_scenario_step "scatter_facet_col"  "point" "y" "double" "x" "double" "CAT1" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[scatter_facet_col]}" \
    "model/columnTable/graphicalFactors/@0/factor" "CAT2" "string"

create_scenario_step "scatter_facet_both" "point" "y" "double" "x" "double" "CAT1" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[scatter_facet_both]}" \
    "model/rowTable/graphicalFactors/@0/factor" "CAT2" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[scatter_facet_both]}" \
    "model/columnTable/graphicalFactors/@0/factor" "CAT3" "string"

# Faceted line variants (CAT1 color, faceted by CAT2/CAT3)
create_scenario_step "line_facet_row"  "line" "y" "double" "x" "double" "CAT1" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[line_facet_row]}" \
    "model/rowTable/graphicalFactors/@0/factor" "CAT2" "string"

create_scenario_step "line_facet_col"  "line" "y" "double" "x" "double" "CAT1" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[line_facet_col]}" \
    "model/columnTable/graphicalFactors/@0/factor" "CAT2" "string"

create_scenario_step "line_facet_both" "line" "y" "double" "x" "double" "CAT1" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[line_facet_both]}" \
    "model/rowTable/graphicalFactors/@0/factor" "CAT2" "string"
patch_factor "$WORKFLOW_ID" "${SCENARIO_STEPS[line_facet_both]}" \
    "model/columnTable/graphicalFactors/@0/factor" "CAT3" "string"

# Transform showcase steps (each uses pre-transformed columns + transform override for axis labels)
create_scenario_step "scatter_tf_log"     "point" "y_log"     "double" "x_log"     "double" "CAT1" "string"
create_scenario_step "scatter_tf_asinh"   "point" "y_asinh"   "double" "x_asinh"   "double" "CAT1" "string"
create_scenario_step "scatter_tf_logicle" "point" "y_logicle" "double" "x_logicle" "double" "CAT1" "string"

# NOTE: scatter_cont (continuous color) and bar charts are not yet supported.
# - scatter_cont: using same column for axis+color causes .colorLevels conflict
# - bar: operator X-axis loading expects .minX/.maxX, bar provides .xLevels

# --- Prepare all steps (create CubeQueryTasks) ---
echo ""
echo "=== Preparing steps (CubeQueryTask creation) ==="

for name in "${!SCENARIO_STEPS[@]}"; do
    prepare_step "$name"
done

# --- Render all combinations ---
echo ""
echo "=== Rendering non-heatmap scenarios (${#BACKENDS[@]} backends x ${#THEMES[@]} themes) ==="

for name in "${!SCENARIO_STEPS[@]}"; do
    if [[ "$name" == "heatmap" || "$name" == *facet* || "$name" == scatter_tf_* ]]; then continue; fi
    render_scenario "$name"
done

echo ""
echo "=== Rendering heatmap (${#BACKENDS[@]} backends x ${#THEMES[@]} themes x ${#HEATMAP_PALETTES[@]} palettes) ==="
render_heatmap_scenario "heatmap"

echo ""
echo "=== Rendering faceted plots (bw theme, cpu backend) ==="
render_faceted "scatter_facet_row"
render_faceted "scatter_facet_col"
render_faceted "scatter_facet_both" "1200" "800"
render_faceted "line_facet_row"
render_faceted "line_facet_col"
render_faceted "line_facet_both" "1200" "800"

echo ""
echo "=== Rendering axis transforms (bw theme, cpu, ${#TRANSFORMS[@]} transforms) ==="
render_transform_combos

echo ""
echo "=== Rendering marker shapes (bw theme, cpu, 26 shapes) ==="
render_marker_shapes

echo ""
echo "=== Rendering opacity levels (bw theme, cpu, ${#OPACITIES[@]} levels) ==="
render_opacity_combos

echo ""
echo "=== Rendering element toggle combos (bw theme, cpu, 16 combos x 3 chart types) ==="
render_toggle_combos "scatter_cat"
render_toggle_combos "line_cat"
render_heatmap_toggle_combos "heatmap"

# --- Generate HTML ---
echo ""
echo "=== Generating showcase ==="
generate_html

# Remove SHOWCASE.md if it exists (replaced by HTML)
rm -f "${SCRIPT_DIR}/SHOWCASE.md"

# --- Cleanup ---
echo ""
echo "=== Cleanup ==="
delete_project "$PROJECT_ID"
rm -f "${SCRIPT_DIR}/operator_config.json"

# --- Summary ---
echo ""
echo "=== DONE ==="
local_count=$(ls -1 "$OUTPUT_DIR"/*.png 2>/dev/null | wc -l)
echo "Generated $local_count images in $OUTPUT_DIR"
echo "Open showcase: $SHOWCASE_HTML"
