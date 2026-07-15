"use client";

/**
 * Renders one backend-suggested chart.
 *
 * The backend picks the *form* by column kind (categorical→bar, numeric→histogram,
 * datetime→time series); this file only draws it. Each chart is a single series ("count"), so
 * per the dataviz rules there's no legend — the card title names the series — and every chart
 * uses the same validated slot (--chart-1) rather than cycling hues for decoration.
 *
 * Bars come straight from the config (`top_values`, computed in ClickHouse). Histogram and
 * time-series buckets are derived client-side from a sampled page of rows, so they're labelled
 * as a sample when the dataset is larger than what we fetched.
 */

import { Bar, BarChart, CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import type { ChartSpec } from "@/lib/api";

const BUCKETS = 12;
const SERIES: ChartConfig = { count: { label: "Rows", color: "var(--chart-1)" } };

type Row = Record<string, unknown>;

/** Numeric values of `column`, ignoring nulls and non-numbers. */
function numbers(rows: Row[], column: string): number[] {
  return rows
    .map((r) => Number(r[column]))
    .filter((n) => Number.isFinite(n));
}

/** Equal-width bins across the observed range. */
function histogram(values: number[]) {
  if (values.length === 0) return [];
  const min = Math.min(...values);
  const max = Math.max(...values);
  if (min === max) return [{ label: format(min), count: values.length }];

  const width = (max - min) / BUCKETS;
  const bins = Array.from({ length: BUCKETS }, (_, i) => ({
    label: format(min + i * width),
    count: 0,
  }));
  for (const v of values) {
    // The max value would land one past the last bin — clamp it in.
    const i = Math.min(Math.floor((v - min) / width), BUCKETS - 1);
    bins[i].count += 1;
  }
  return bins;
}

/** Rows per day for a temporal column. */
function perDay(rows: Row[], column: string) {
  const counts = new Map<string, number>();
  for (const r of rows) {
    const raw = r[column];
    if (raw == null) continue;
    const d = new Date(String(raw));
    if (Number.isNaN(d.getTime())) continue;
    const day = d.toISOString().slice(0, 10);
    counts.set(day, (counts.get(day) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([label, count]) => ({ label, count }));
}

function format(n: number) {
  return Number.isInteger(n) ? String(n) : n.toFixed(2);
}

export function DashboardChart({
  spec,
  rows,
  sampled,
}: {
  spec: ChartSpec;
  rows: Row[];
  sampled: boolean;
}) {
  const data =
    spec.kind === "bar"
      ? (spec.top_values ?? []).map((t) => ({ label: t.value, count: t.count }))
      : spec.kind === "histogram"
        ? histogram(numbers(rows, spec.column))
        : perDay(rows, spec.column);

  // A chart with nothing to draw should say so, not render empty axes.
  if (data.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{spec.title}</CardTitle>
        </CardHeader>
        <CardContent className="py-8 text-center text-sm text-muted-foreground">
          Not enough data to chart this column.
        </CardContent>
      </Card>
    );
  }

  // Bars/histograms are derived from a sample only for the non-bar forms.
  const note =
    spec.kind === "bar"
      ? `Top ${data.length} values by row count`
      : sampled
        ? "Based on a sample of the first rows"
        : "All rows";

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{spec.title}</CardTitle>
        <CardDescription>{note}</CardDescription>
      </CardHeader>
      <CardContent>
        <ChartContainer config={SERIES} className="h-56 w-full">
          {spec.kind === "time_series" ? (
            <LineChart data={data} margin={{ left: 4, right: 8, top: 4 }}>
              <CartesianGrid vertical={false} strokeOpacity={0.4} />
              <XAxis
                dataKey="label"
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                minTickGap={24}
              />
              <YAxis tickLine={false} axisLine={false} width={36} allowDecimals={false} />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Line
                dataKey="count"
                type="monotone"
                stroke="var(--color-count)"
                strokeWidth={2}
                dot={false}
                activeDot={{ r: 4 }}
              />
            </LineChart>
          ) : (
            <BarChart data={data} margin={{ left: 4, right: 8, top: 4 }}>
              <CartesianGrid vertical={false} strokeOpacity={0.4} />
              <XAxis
                dataKey="label"
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                interval="preserveStartEnd"
              />
              <YAxis tickLine={false} axisLine={false} width={36} allowDecimals={false} />
              <ChartTooltip content={<ChartTooltipContent />} />
              {/* 4px rounded data-end, anchored to the baseline. */}
              <Bar dataKey="count" fill="var(--color-count)" radius={[4, 4, 0, 0]} />
            </BarChart>
          )}
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
