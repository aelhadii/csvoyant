"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { JobStatusBadge } from "@/components/job-status-badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ApiException, createJob, isTerminal, listJobs, type Job } from "@/lib/api";
import { useRequireAuth } from "@/lib/auth";

/** How often to re-poll while any job is still in flight. */
const POLL_MS = 2000;

export default function JobsPage() {
  const { user, loading: authLoading } = useRequireAuth();
  const [jobs, setJobs] = useState<Job[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const load = useCallback(async () => {
    try {
      setJobs(await listJobs());
      setError(null);
    } catch (err) {
      setError(err instanceof ApiException ? err.error.message : "Could not load jobs.");
    }
  }, []);

  useEffect(() => {
    if (user) void load();
  }, [user, load]);

  // Poll only while something is actually moving — a settled list costs nothing.
  useEffect(() => {
    if (!jobs?.some((j) => !isTerminal(j.status))) return;
    const t = setInterval(() => void load(), POLL_MS);
    return () => clearInterval(t);
  }, [jobs, load]);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    try {
      await createJob(url.trim());
      setUrl("");
      toast.success("Job submitted", { description: "Ingestion has started." });
      await load();
    } catch (err) {
      toast.error("Could not submit", {
        description: err instanceof ApiException ? err.error.message : "Something went wrong.",
      });
    } finally {
      setSubmitting(false);
    }
  }

  if (authLoading || !user) return <ListSkeleton />;

  return (
    <div className="space-y-8">
      <Card>
        <CardHeader>
          <CardTitle>Submit a dataset</CardTitle>
          <CardDescription>
            Paste a direct URL to a data file (CSV, TSV, Parquet, JSON — .gz/.xz/.zst are fine).
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="flex flex-col gap-3 sm:flex-row">
            <Input
              type="url"
              required
              placeholder="https://example.com/data.csv"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="flex-1"
            />
            <Button type="submit" disabled={submitting || !url.trim()}>
              {submitting ? "Submitting…" : "Ingest"}
            </Button>
          </form>
        </CardContent>
      </Card>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold tracking-tight">Your jobs</h2>

        {error && <p className="text-sm text-destructive">{error}</p>}

        {!jobs ? (
          <ListSkeleton />
        ) : jobs.length === 0 ? (
          <Card>
            <CardContent className="py-10 text-center text-sm text-muted-foreground">
              No jobs yet — submit a URL above to build your first dashboard.
            </CardContent>
          </Card>
        ) : (
          <JobsTable jobs={jobs} />
        )}
      </section>
    </div>
  );
}

function JobsTable({ jobs }: { jobs: Job[] }) {
  return (
    <div className="rounded-md border">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Source</TableHead>
            <TableHead className="w-32">Status</TableHead>
            <TableHead className="w-28 text-right">Rows</TableHead>
            <TableHead className="w-44">Created</TableHead>
            <TableHead className="w-24" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {jobs.map((job) => (
            <TableRow key={job.id}>
              <TableCell className="max-w-sm">
                <span className="block truncate font-mono text-xs" title={job.source_url}>
                  {job.source_url}
                </span>
                {job.error && (
                  <span className="mt-1 block text-xs text-destructive">{job.error}</span>
                )}
              </TableCell>
              <TableCell>
                <JobStatusBadge status={job.status} />
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {job.row_count?.toLocaleString() ?? "—"}
              </TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {new Date(job.created_at).toLocaleString()}
              </TableCell>
              <TableCell className="text-right">
                {job.status === "ready" && (
                  <Button variant="ghost" size="sm" render={<Link href={`/jobs/${job.id}`}>View</Link>} />
                )}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

function ListSkeleton() {
  return (
    <div className="space-y-3">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-12 w-full" />
      ))}
    </div>
  );
}
