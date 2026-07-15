"use client";

/** Paginated, sortable view of a job's ingested rows (server-side paging via /jobs/{id}/data). */

import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ApiException, getData, type ColumnMeta, type DataPage } from "@/lib/api";

const PAGE_SIZE = 25;

export function DataTable({ jobId, columns }: { jobId: string; columns: ColumnMeta[] }) {
  const [page, setPage] = useState(1);
  const [sort, setSort] = useState<string | null>(null);
  const [order, setOrder] = useState<"asc" | "desc">("asc");
  const [data, setData] = useState<DataPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setData(
        await getData(jobId, {
          page,
          pageSize: PAGE_SIZE,
          sort: sort ?? undefined,
          order: sort ? order : undefined,
        }),
      );
      setError(null);
    } catch (err) {
      setError(err instanceof ApiException ? err.error.message : "Could not load rows.");
    } finally {
      setLoading(false);
    }
  }, [jobId, page, sort, order]);

  useEffect(() => {
    void load();
  }, [load]);

  function toggleSort(column: string) {
    if (sort !== column) {
      setSort(column);
      setOrder("asc");
    } else if (order === "asc") {
      setOrder("desc");
    } else {
      // Third click clears sorting rather than cycling forever.
      setSort(null);
    }
    setPage(1);
  }

  const totalPages = data ? Math.max(1, Math.ceil(data.total / data.page_size)) : 1;

  if (error) return <p className="text-sm text-destructive">{error}</p>;
  if (!data && loading) return <Skeleton className="h-64 w-full" />;

  return (
    <div className="space-y-3">
      <div className="rounded-md border">
        <div className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((c) => (
                  <TableHead key={c.name} className="whitespace-nowrap">
                    <button
                      type="button"
                      onClick={() => toggleSort(c.name)}
                      className="inline-flex items-center gap-1 hover:text-foreground"
                    >
                      {c.name}
                      {sort === c.name ? (
                        order === "asc" ? (
                          <ArrowUp className="size-3" />
                        ) : (
                          <ArrowDown className="size-3" />
                        )
                      ) : (
                        <ChevronsUpDown className="size-3 opacity-40" />
                      )}
                    </button>
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {data && data.rows.length > 0 ? (
                data.rows.map((row, i) => (
                  <TableRow key={i}>
                    {columns.map((c) => (
                      <TableCell key={c.name} className="whitespace-nowrap tabular-nums">
                        {row[c.name] === null || row[c.name] === undefined
                          ? "—"
                          : String(row[c.name])}
                      </TableCell>
                    ))}
                  </TableRow>
                ))
              ) : (
                <TableRow>
                  <TableCell
                    colSpan={columns.length}
                    className="py-10 text-center text-sm text-muted-foreground"
                  >
                    No rows to show.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </div>
      </div>

      <div className="flex items-center justify-between text-sm text-muted-foreground">
        <span>
          {data ? `${data.total.toLocaleString()} rows · page ${data.page} of ${totalPages}` : null}
        </span>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={page <= 1 || loading}
            onClick={() => setPage((p) => p - 1)}
          >
            Previous
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={page >= totalPages || loading}
            onClick={() => setPage((p) => p + 1)}
          >
            Next
          </Button>
        </div>
      </div>
    </div>
  );
}
