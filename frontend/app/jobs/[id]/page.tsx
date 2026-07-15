"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { useEffect, useState } from "react";

import { DashboardChart } from "@/components/dashboard-charts";
import { DataTable } from "@/components/data-table";
import { JobStatusBadge } from "@/components/job-status-badge";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  ApiException,
  getDashboard,
  getData,
  getJob,
  isTerminal,
  type Dashboard,
  type Job,
} from "@/lib/api";
import { useRequireAuth } from "@/lib/auth";
import { cn } from "@/lib/utils";

/** Rows pulled once to derive histogram / time-series buckets client-side. */
const CHART_SAMPLE = 500;
/** Re-poll cadence while the job is still in flight. */
const POLL_MS = 2000;

export default function JobDashboardPage() {
  const { user, loading: authLoading } = useRequireAuth();
  const params = useParams<{ id: string }>();
  const id = params.id;

  const [job, setJob] = useState<Job | null>(null);
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [sample, setSample] = useState<Record<string, unknown>[]>([]);
  const [sampled, setSampled] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Follow the job until it settles, then stop. Self-scheduling (rather than setInterval) so
  // reaching a terminal state genuinely ends the polling instead of re-arming it.
  useEffect(() => {
    if (!user || !id) return;
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function tick() {
      try {
        const j = await getJob(id);
        if (stop) return;
        setJob(j);

        if (j.status === "ready") {
          const [d, page] = await Promise.all([
            getDashboard(id),
            getData(id, { page: 1, pageSize: CHART_SAMPLE }),
          ]);
          if (stop) return;
          setDashboard(d);
          setSample(page.rows);
          setSampled(page.total > page.rows.length);
        }

        // Nothing more will change once terminal — don't schedule another poll.
        if (!stop && !isTerminal(j.status)) timer = setTimeout(tick, POLL_MS);
      } catch (err) {
        if (!stop) {
          setError(err instanceof ApiException ? err.error.message : "Could not load this job.");
        }
      }
    }

    void tick();
    return () => {
      stop = true;
      if (timer) clearTimeout(timer);
    };
  }, [user, id]);

  if (authLoading || !user) return <PageSkeleton />;

  if (error) {
    return (
      <Alert variant="destructive">
        <AlertTitle>Unavailable</AlertTitle>
        <AlertDescription>{error}</AlertDescription>
      </Alert>
    );
  }

  if (!job) return <PageSkeleton />;

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <div className="flex items-center gap-3">
            <h1 className="text-xl font-semibold tracking-tight">Dashboard</h1>
            <JobStatusBadge status={job.status} />
          </div>
          <p className="font-mono text-xs break-all text-muted-foreground">{job.source_url}</p>
        </div>
        <Button variant="outline" size="sm" render={<Link href="/jobs">Back to jobs</Link>} />
      </div>

      {job.status === "failed" ? (
        <Alert variant="destructive">
          <AlertTitle>Ingestion failed</AlertTitle>
          <AlertDescription>{job.error ?? "No reason was recorded."}</AlertDescription>
        </Alert>
      ) : !isTerminal(job.status) ? (
        <Card>
          <CardContent className="space-y-3 py-10 text-center">
            <p className="text-sm text-muted-foreground">
              Ingestion is <span className="font-medium text-foreground">{job.status}</span> — this
              page updates automatically.
            </p>
            <Skeleton className="mx-auto h-2 w-40" />
          </CardContent>
        </Card>
      ) : !dashboard ? (
        <PageSkeleton />
      ) : (
        <>
          <section className="grid gap-4 sm:grid-cols-3">
            <Stat label="Rows" value={dashboard.summary.row_count.toLocaleString()} />
            <Stat label="Columns" value={String(dashboard.summary.column_count)} />
            <Stat
              label="Ingested"
              value={job.finished_at ? new Date(job.finished_at).toLocaleString() : "—"}
            />
          </section>

          {dashboard.charts.length > 0 && (
            // A lone chart shouldn't sit in a half-empty two-column row.
            <section
              className={cn("grid gap-4", dashboard.charts.length > 1 && "lg:grid-cols-2")}
            >
              {dashboard.charts.map((spec) => (
                <DashboardChart
                  key={`${spec.kind}-${spec.column}`}
                  spec={spec}
                  rows={sample}
                  sampled={sampled}
                />
              ))}
            </section>
          )}

          <section className="space-y-3">
            <h2 className="text-lg font-semibold tracking-tight">Columns</h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {dashboard.columns.map((c) => (
                <Card key={c.name}>
                  <CardHeader className="pb-2">
                    <CardTitle className="truncate text-sm" title={c.name}>
                      {c.name}
                    </CardTitle>
                    <p className="font-mono text-xs text-muted-foreground">{c.type}</p>
                  </CardHeader>
                  <CardContent className="space-y-1 text-xs text-muted-foreground">
                    {c.stats.min !== undefined && c.stats.min !== null && (
                      <StatLine label="min" value={c.stats.min} />
                    )}
                    {c.stats.max !== undefined && c.stats.max !== null && (
                      <StatLine label="max" value={c.stats.max} />
                    )}
                    {c.stats.avg !== undefined && c.stats.avg !== null && (
                      <StatLine label="avg" value={Number(c.stats.avg).toFixed(2)} />
                    )}
                    {c.stats.distinct !== undefined && (
                      <StatLine label="distinct" value={c.stats.distinct} />
                    )}
                    {c.stats.nulls !== undefined && <StatLine label="nulls" value={c.stats.nulls} />}
                  </CardContent>
                </Card>
              ))}
            </div>
          </section>

          <section className="space-y-3">
            <h2 className="text-lg font-semibold tracking-tight">Rows</h2>
            <DataTable jobId={id} columns={dashboard.columns} />
          </section>
        </>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <Card>
      <CardContent className="pt-6">
        <p className="text-sm text-muted-foreground">{label}</p>
        <p className="mt-1 text-2xl font-semibold tabular-nums">{value}</p>
      </CardContent>
    </Card>
  );
}

function StatLine({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex justify-between gap-2">
      <span>{label}</span>
      <span className="font-medium tabular-nums text-foreground">{String(value)}</span>
    </div>
  );
}

function PageSkeleton() {
  return (
    <div className="space-y-6">
      <Skeleton className="h-8 w-48" />
      <div className="grid gap-4 sm:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-24 w-full" />
        ))}
      </div>
      <Skeleton className="h-64 w-full" />
    </div>
  );
}
