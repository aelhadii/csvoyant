"use client";

/**
 * Admin view: every user's jobs. The API already returns all jobs to an Admin from `GET /jobs`
 * (and only the caller's to a User), so this page is the same call — the server, not the UI, is
 * what enforces the boundary. Hiding it from non-admins here is only to avoid a pointless page.
 */

import Link from "next/link";
import { useEffect, useState } from "react";

import { JobStatusBadge } from "@/components/job-status-badge";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ApiException, isTerminal, listJobs, type Job } from "@/lib/api";
import { useRequireAuth } from "@/lib/auth";

export default function AdminPage() {
  const { user, loading } = useRequireAuth();
  const [jobs, setJobs] = useState<Job[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!user || user.role !== "admin") return;
    let stop = false;
    async function load() {
      try {
        const all = await listJobs();
        if (!stop) {
          setJobs(all);
          setError(null);
        }
      } catch (err) {
        if (!stop) {
          setError(err instanceof ApiException ? err.error.message : "Could not load jobs.");
        }
      }
    }
    void load();
    return () => {
      stop = true;
    };
  }, [user]);

  // Poll only while something is actually moving — a settled list needs no refresh.
  useEffect(() => {
    if (user?.role !== "admin") return;
    if (!jobs?.some((j) => !isTerminal(j.status))) return;
    const t = setInterval(async () => {
      try {
        setJobs(await listJobs());
      } catch {
        /* transient; the next tick retries */
      }
    }, 3000);
    return () => clearInterval(t);
  }, [user, jobs]);

  if (loading || !user) return <Skeleton className="h-64 w-full" />;

  if (user.role !== "admin") {
    return (
      <Alert variant="destructive">
        <AlertTitle>Admins only</AlertTitle>
        <AlertDescription>This area requires the admin role.</AlertDescription>
      </Alert>
    );
  }

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">All jobs</h1>
        <p className="text-sm text-muted-foreground">Every user&apos;s ingestion jobs.</p>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      {!jobs ? (
        <Skeleton className="h-40 w-full" />
      ) : jobs.length === 0 ? (
        <Card>
          <CardContent className="py-10 text-center text-sm text-muted-foreground">
            No jobs have been submitted yet.
          </CardContent>
        </Card>
      ) : (
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
                    {isTerminal(job.status) && job.status === "ready" && (
                      <Button variant="ghost" size="sm" render={<Link href={`/jobs/${job.id}`}>View</Link>} />
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
