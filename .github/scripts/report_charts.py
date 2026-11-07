"""Small Markdown charts shared by CI and release job summaries."""

import math


def pie_chart(title, values):
    slices = [(label, value) for label, value in values
              if isinstance(value, (int, float)) and math.isfinite(value) and value > 0]
    if not slices:
        return f"### {title}\n\nNo measured data available.\n"
    lines = [f"### {title}", "", "```mermaid", "pie showData", f'    title {title}']
    lines.extend(f'    "{label}" : {value}' for label, value in slices)
    lines.extend(["```", ""])
    return "\n".join(lines)


def bar_chart(title, values, unit="s"):
    measured = [(label, float(value)) for label, value in values
                if isinstance(value, (int, float)) and math.isfinite(value) and value >= 0]
    if not measured:
        return f"### {title}\n\nNo measured data available.\n"
    maximum = max(value for _, value in measured)
    lines = [f"### {title}", "", f"| Measure | {unit} | Chart |", "| --- | ---: | --- |"]
    for label, value in measured:
        width = max(1, round(value / maximum * 20)) if maximum else 0
        lines.append(f"| {label} | {value:.3f} | {'█' * width} |")
    return "\n".join(lines) + "\n"
