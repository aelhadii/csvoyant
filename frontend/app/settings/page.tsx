"use client";

import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiException, changeEmail } from "@/lib/api";
import { useAuth, useRequireAuth } from "@/lib/auth";

export default function SettingsPage() {
  const { user, loading } = useRequireAuth();
  const { setUser } = useAuth();
  const [newEmail, setNewEmail] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    try {
      // The API re-authenticates the change with the current password.
      const updated = await changeEmail(newEmail.trim(), password);
      setUser(updated);
      setNewEmail("");
      setPassword("");
      toast.success("Email updated", { description: `Now signed in as ${updated.email}.` });
    } catch (err) {
      toast.error("Could not update email", {
        description: err instanceof ApiException ? err.error.message : "Something went wrong.",
      });
    } finally {
      setSubmitting(false);
    }
  }

  if (loading || !user) return <Skeleton className="h-64 w-full max-w-md" />;

  return (
    <div className="max-w-md space-y-6">
      <h1 className="text-xl font-semibold tracking-tight">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Change email</CardTitle>
          <CardDescription>
            Currently <span className="font-medium text-foreground">{user.email}</span>.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="new-email">New email</Label>
              <Input
                id="new-email"
                type="email"
                required
                value={newEmail}
                onChange={(e) => setNewEmail(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="current-password">Current password</Label>
              <Input
                id="current-password"
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </div>
            <Button type="submit" disabled={submitting || !newEmail.trim() || !password}>
              {submitting ? "Updating…" : "Update email"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
