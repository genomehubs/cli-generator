#!/bin/bash

OUTDIR=~/reports
mkdir -p "$OUTDIR"
FORMAT=png
AXES="assembly_level assembly_span assembly_date genus"
THRESHOLDS="10 1000"
MODES="stacked grouped facet"
CUMULATIVE="true false"


function vl_convert() {
  local format="$1"
  local output_file="$2"
  if [[ "$format" == "svg" ]]; then
    python3 -c "import vl_convert as vlc, json, sys; spec=json.loads(sys.stdin.read()); print(vlc.vegalite_to_svg(spec), end='')" > "$output_file"
  else
    python3 -c "import vl_convert as vlc, json, sys; spec=json.loads(sys.stdin.read()); sys.stdout.buffer.write(vlc.vegalite_to_png(spec))" > "$output_file"
  fi
}

# # Test histogram report with different axis types
# for x_axis in $AXES; do
#   echo "Testing histogram with x=$x_axis"
#   curl -s -X POST 'http://localhost:3000/api/v3/report' -H 'accept: application/json' -H 'Content-Type: application/json' -d "{\"query\":{\"index\":\"taxon\", \"taxa\": [\"canidae\"], \"taxon_filter_type\": \"tree\"},\"params\":{},\"report\":{\"report\":\"histogram\",\"x\":\"$x_axis\",\"bucket_count\":20},\"include_plot_spec\":true,\"display\":{\"title\":\"histogram test\"}}" \
#   | cargo run --quiet --bin plot_to_vl \
#   | vl_convert "$FORMAT" "$OUTDIR/histogram_${x_axis}.$FORMAT"
# done

# Test histogram report with different axis and mode combinations
category_axis="assembly_level"
for x_axis in $AXES; do
  for mode in $MODES; do
    for cumulative in $CUMULATIVE; do
      echo "Testing histogram with x=$x_axis, mode=$mode, cumulative=$cumulative"
      curl -s -X POST 'http://localhost:3000/api/v3/report' -H 'accept: application/json' -H 'Content-Type: application/json' -d "{\"query\":{\"index\":\"taxon\", \"taxa\": [\"canidae\"], \"taxon_filter_type\": \"tree\"},\"params\":{},\"report\":{\"report\":\"histogram\",\"x\":\"$x_axis\",\"cat\":\"$category_axis\",\"bucket_count\":20},\"include_plot_spec\":true,\"display\":{\"title\":\"histogram test\",\"histogram\":{\"mode\":\"$mode\",\"cumulative\":$cumulative}}}" \
      | cargo run --quiet --bin plot_to_vl \
      | vl_convert "$FORMAT" "$OUTDIR/histogram_${x_axis}_${mode}_cumulative_${cumulative}.$FORMAT"
    done
  done
done



exit;

# Test histogram report with different axis and category combinations
for x_axis in $AXES; do
  for cat_axis in $AXES; do
    echo "Testing histogram with x=$x_axis, category=$cat_axis"
    curl -s -X POST 'http://localhost:3000/api/v3/report' -H 'accept: application/json' -H 'Content-Type: application/json' -d "{\"query\":{\"index\":\"taxon\", \"taxa\": [\"canidae\"], \"taxon_filter_type\": \"tree\"},\"params\":{},\"report\":{\"report\":\"histogram\",\"x\":\"$x_axis\",\"cat\":\"$cat_axis\",\"bucket_count\":20},\"include_plot_spec\":true,\"display\":{\"title\":\"histogram test\"}}" \
    | cargo run --quiet --bin plot_to_vl \
    | vl_convert "$FORMAT" "$OUTDIR/histogram_${x_axis}_by_${cat_axis}.$FORMAT"
  done
done


exit;


# Test scatter report with different axis combinations and thresholds

for x_axis in $AXES; do
  for y_axis in $AXES; do
    for threshold in $THRESHOLDS; do
      echo "Testing scatter with x=$x_axis, y=$y_axis, threshold=$threshold"
      shape=$(if [[ "$threshold" -le 10 ]]; then echo "rect"; else echo "point"; fi)
      curl -s -X POST 'http://localhost:3000/api/v3/report' -H 'accept: application/json' -H 'Content-Type: application/json' -d "{\"query\":{\"index\":\"taxon\", \"taxa\": [\"canidae\"], \"taxon_filter_type\": \"tree\"},\"params\":{},\"report\":{\"report\":\"scatter\",\"x\":\"$x_axis\",\"y\":\"$y_axis\",\"scatter_threshold\":$threshold},\"include_plot_spec\":true,\"display\":{\"title\":\"scatter test\"}}" \
      | cargo run --quiet --bin plot_to_vl \
      | vl_convert "$FORMAT" "$OUTDIR/scatter_${shape}_${x_axis}_${y_axis}.$FORMAT"
    done
  done
done
