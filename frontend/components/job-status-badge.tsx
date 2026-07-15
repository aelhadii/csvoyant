import { Badge } from "@/components/ui/badge";
import type { JobStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

/** In-flight stages read as "working", ready as success, failed as destructive. */
const styles: Record<JobStatus, string> = {
  queued: "bg-muted text-muted-foreground",
  downloading: "bg-blue-100 text-blue-800 dark:bg-blue-950 dark:text-blue-300",
  inferring: "bg-blue-100 text-blue-800 dark:bg-blue-950 dark:text-blue-300",
  ingesting: "bg-blue-100 text-blue-800 dark:bg-blue-950 dark:text-blue-300",
  ready: "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300",
  failed: "bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-300",
};

export function JobStatusBadge({ status }: { status: JobStatus }) {
  return (
    <Badge variant="secondary" className={cn("border-transparent capitalize", styles[status])}>
      {status}
    </Badge>
  );
}
