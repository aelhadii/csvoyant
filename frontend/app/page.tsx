"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";

import { useAuth } from "@/lib/auth";

/** Entry point: straight to the jobs list when signed in, otherwise to sign-in. */
export default function Home() {
  const { user, loading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (loading) return;
    router.replace(user ? "/jobs" : "/login");
  }, [user, loading, router]);

  return null;
}
