#!/usr/bin/env python3
"""Generate charts from categorized query logs."""
import os
import glob
import json
import dotenv
import clickhouse_connect
import pandas as pd
import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
import numpy as np
from matplotlib.patches import Patch

# Load environment
dotenv.load_dotenv()
CLICKHOUSE_HOST = os.getenv("CLICKHOUSE_HOST")
CLICKHOUSE_USER = os.getenv("CLICKHOUSE_USER")
CLICKHOUSE_PASSWORD = os.getenv("CLICKHOUSE_PASSWORD")
CLICKHOUSE_DATABASE = os.getenv("CLICKHOUSE_DATABASE")

# Category colors
CATEGORY_COLORS = {
    "explore": "#4C72B0",
    "implement": "#55A868",
    "test": "#C44E52",
    "debug": "#DD8452",
    "prompt": "#8172B2",
}
DEFAULT_COLOR = "#AAAAAA"
CATEGORY_ORDER = ["explore", "implement", "test", "debug", "prompt", "unknown"]


def get_client():
    """Create ClickHouse client."""
    _ch_host, _ch_port = CLICKHOUSE_HOST.split(":")
    return clickhouse_connect.get_client(
        host=_ch_host,
        port=int(_ch_port),
        username=CLICKHOUSE_USER,
        password=CLICKHOUSE_PASSWORD,
        database=CLICKHOUSE_DATABASE,
    )


def query_clickhouse(client, sql):
    """Execute query and return results as list of dicts."""
    result = client.query(sql)
    columns = result.column_names
    return [dict(zip(columns, row)) for row in result.result_rows]


def find_json_files(pattern="01*.json"):
    """Find all JSON files matching pattern."""
    return sorted(glob.glob(pattern))


def load_categories(categories_file="categories.json"):
    """Load categories mapping."""
    with open(categories_file) as f:
        data = json.load(f)
    # Filter out empty categories
    return {k: v for k, v in data.items() if v}


def fetch_data_for_range(client, start_id, end_id):
    """Fetch query log data for given ID range."""
    sql = f"""
    SELECT
        id,
        model,
        duration_completed,
        input_token,
        output_token,
        cache_token,
        response_payload.id AS response_id
    FROM query_logs
    WHERE id >= {start_id} AND id <= {end_id}
    ORDER BY id ASC
    """
    return query_clickhouse(client, sql)


def generate_chart(df, categories_map, output_path):
    """Generate the chart and save to file."""
    # Prepare data
    df_with_cat = df.copy()
    df_with_cat['category'] = df_with_cat['id'].apply(
        lambda x: categories_map.get(str(x), 'unknown')
    )

    # Token by category
    token_by_cat = df_with_cat.groupby('category').agg({
        'input_token': 'sum',
        'output_token': 'sum'
    }).reset_index()
    token_by_cat['total'] = token_by_cat['input_token'] + \
        token_by_cat['output_token']
    token_by_cat['input_pct'] = token_by_cat['input_token'] / \
        token_by_cat['total'] * 100
    token_by_cat['output_pct'] = token_by_cat['output_token'] / \
        token_by_cat['total'] * 100
    token_by_cat['sort_key'] = token_by_cat['category'].apply(
        lambda x: CATEGORY_ORDER.index(
            x) if x in CATEGORY_ORDER else len(CATEGORY_ORDER)
    )
    token_by_cat = token_by_cat.sort_values(
        'sort_key', ascending=False).reset_index(drop=True)

    # Duration by category
    duration_by_cat = df_with_cat.groupby(
        'category')['duration_completed'].sum().reset_index()
    total_dur = duration_by_cat['duration_completed'].sum()
    duration_by_cat['pct'] = duration_by_cat['duration_completed'] / \
        total_dur * 100
    duration_by_cat['sort_key'] = duration_by_cat['category'].apply(
        lambda x: CATEGORY_ORDER.index(
            x) if x in CATEGORY_ORDER else len(CATEGORY_ORDER)
    )
    duration_by_cat = duration_by_cat.sort_values(
        'sort_key', ascending=False).reset_index(drop=True)

    # Create figure
    fig = plt.figure(figsize=(14, 16))
    gs = gridspec.GridSpec(5, 1, height_ratios=[4, 4, 0.5, 2, 2], hspace=0.35)

    ax_tok = fig.add_subplot(gs[0])
    ax_dur = fig.add_subplot(gs[1], sharex=ax_tok)
    ax_cat = fig.add_subplot(gs[2], sharex=ax_tok)
    ax_tok_cat = fig.add_subplot(gs[3])
    ax_dur_cat = fig.add_subplot(gs[4])

    # Summary statistics
    df_valid = df[df['duration_completed'] > 0]
    total_duration = df['duration_completed'].sum()
    total_input = df['input_token'].sum()
    total_output = df['output_token'].sum()
    max_cache = df['cache_token'].max()
    total_tokens = total_input + total_output

    x = list(range(len(df)))
    x_labels = [str(rid)[:8] if pd.notna(rid) else str(i)
                for i, rid in enumerate(df['id'].values)]

    # Category per row
    row_categories = [categories_map.get(
        str(r_id), None) for r_id in df['id'].values]
    bar_colors = [CATEGORY_COLORS.get(cat, DEFAULT_COLOR)
                  for cat in row_categories]

    # Token rate
    token_rate = np.array([np.nan if d == 0 else o / (d / 1000.0)
                           for d, o in zip(df['duration_completed'].values, df['output_token'].values)])

    # Tokens per request
    bar_width = 0.6
    ax_tok.bar(x, df['input_token'].values, bar_width,
               label='Input Tokens', color='green', alpha=0.8)
    ax_tok.bar(x, df['output_token'].values, bar_width, bottom=df['input_token'].values,
               label='Output Tokens', color='orange', alpha=0.8)
    ax_tok.bar(x, df['cache_token'].values, bar_width,
               bottom=df['input_token'].values + df['output_token'].values,
               label='Cache Tokens', color='purple', alpha=0.8)
    ax_tok.set_ylabel('Tokens')
    ax_tok.set_title('Tokens per Request')
    ax_tok.legend(loc='upper right')
    plt.setp(ax_tok.get_xticklabels(), visible=False)

    # Summary box on token chart
    avg_token_rate = df_valid['output_token'].sum(
    ) / (df_valid['duration_completed'].sum() / 1000) if len(df_valid) > 0 else 0
    summary_text = f"""Total Duration: {total_duration:,} ms
Total Input Tokens: {total_input:,}
Total Output Tokens: {total_output:,}
Max Cache Tokens: {max_cache:,}
Total Tokens: {total_tokens:,}
Avg Output Token Rate: {avg_token_rate:.1f} tokens/sec
Requests: {len(df)} (valid: {len(df_valid)})"""
    props = dict(boxstyle='round', facecolor='wheat', alpha=0.9)
    ax_tok.text(0.02, 0.98, summary_text, transform=ax_tok.transAxes, fontsize=10,
                verticalalignment='top', bbox=props, family='monospace', zorder=100)

    # Duration
    ax_dur.bar(x, df['duration_completed'].values,
               color=bar_colors, alpha=0.85)
    ax_dur.set_ylabel('Duration (ms)')
    ax_dur.set_title('Duration Completed per Request')
    ax_dur.set_xticks(x)
    ax_dur.set_xticklabels(x_labels, rotation=45, ha='right', fontsize=8)

    # Token rate on secondary y-axis
    ax_dur2 = ax_dur.twinx()
    valid_x = [i for i in x if not np.isnan(token_rate[i])]
    valid_rate = [token_rate[i] for i in x if not np.isnan(token_rate[i])]
    ax_dur2.plot(valid_x, valid_rate, color='black', marker='o', linewidth=2, markersize=4,
                 linestyle='--', alpha=0.6, label='Token Rate')
    ax_dur2.set_ylabel('Token Rate (tokens/sec)', color='gray')
    ax_dur2.tick_params(axis='y', labelcolor='gray')

    # Legend: categories + token rate
    legend_patches = [Patch(facecolor=c, label=cat, alpha=0.85)
                      for cat, c in CATEGORY_COLORS.items()]
    legend_patches.append(Patch(facecolor=DEFAULT_COLOR,
                          label='unknown', alpha=0.85))
    rate_handle = plt.Line2D([0], [0], color='black', linestyle='--',
                             marker='o', markersize=4, alpha=0.6, label='Token Rate')
    ax_dur.legend(handles=legend_patches +
                  [rate_handle], loc='upper right', fontsize=8, title='Category')

    # Category strip
    for i, cat in enumerate(row_categories):
        color = CATEGORY_COLORS.get(cat, DEFAULT_COLOR)
        ax_cat.bar(i, 1, color=color, alpha=0.85, width=0.8)
        label = (cat or '?')[:3].upper()
        ax_cat.text(i, 0.5, label, ha='center', va='center',
                    fontsize=7, color='white', fontweight='bold')
    ax_cat.set_ylim(0, 1)
    ax_cat.set_yticks([])
    ax_cat.set_xticks([])
    ax_cat.set_ylabel('Cat.', fontsize=8)
    for spine in ['top', 'right', 'bottom']:
        ax_cat.spines[spine].set_visible(False)

    # Token usage by category - horizontal stacked bar
    y_pos = range(len(token_by_cat))
    ax_tok_cat.barh(
        y_pos, token_by_cat['input_token'], color='green', alpha=0.8, label='Input')
    ax_tok_cat.barh(y_pos, token_by_cat['output_token'],
                    left=token_by_cat['input_token'], color='orange', alpha=0.8, label='Output')
    ax_tok_cat.set_yticks(y_pos)
    ax_tok_cat.set_yticklabels(token_by_cat['category'])
    ax_tok_cat.set_xlabel('Tokens')
    ax_tok_cat.set_title('Token Usage by Category')
    ax_tok_cat.legend(loc='lower right')
    for i, row in token_by_cat.iterrows():
        ax_tok_cat.text(row['total'] + row['total'] * 0.02, i,
                        f"I:{row['input_pct']:.0f}% O:{row['output_pct']:.0f}%", va='center', fontsize=9)

    # Duration breakdown by category - horizontal bar with percentage
    ax_dur_cat.barh(y_pos, duration_by_cat['duration_completed'],
                    color=[CATEGORY_COLORS.get(c, DEFAULT_COLOR) for c in duration_by_cat['category']], alpha=0.85)
    ax_dur_cat.set_yticks(y_pos)
    ax_dur_cat.set_yticklabels(duration_by_cat['category'])
    ax_dur_cat.set_xlabel('Duration (ms)')
    ax_dur_cat.set_title('Duration Breakdown by Category')
    for i, row in duration_by_cat.iterrows():
        ax_dur_cat.text(row['duration_completed'] + row['duration_completed']
                        * 0.02, i, f"{row['pct']:.1f}%", va='center', fontsize=9)

    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    print(f"Saved chart to {output_path}")


def main():
    """Main function."""
    # Find all task JSON files
    json_files = find_json_files("01*.json")
    print(
        f"Found {len(json_files)} task files: {[os.path.basename(f) for f in json_files]}")

    # Load categories
    categories_map = load_categories()
    print(f"Loaded {len(categories_map)} categories")

    # Connect to ClickHouse
    client = get_client()

    # Process each task file separately
    for json_file in json_files:
        with open(json_file) as f:
            task = json.load(f)

        start_id = task.get("startId")
        end_id = task.get("endId")

        if start_id and end_id:
            # Generate output filename from json filename (e.g., 01-frontend-button-fix-claude.json -> 01-frontend-button-fix-claude.png)
            base_name = os.path.splitext(os.path.basename(json_file))[0]
            output_path = f"{base_name}.png"

            print(f"Processing {base_name}: {start_id} - {end_id}")
            data = fetch_data_for_range(client, start_id, end_id)
            print(f"  Got {len(data)} records")

            if len(data) == 0:
                print(f"  No data found!")
                continue

            df = pd.DataFrame(data)
            generate_chart(df, categories_map, output_path)


if __name__ == "__main__":
    main()
