"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";

import { Button } from "@/components/ui/button";
import { useAuth } from "@/lib/auth";
import { cn } from "@/lib/utils";

const links = [
  { href: "/jobs", label: "Jobs" },
  { href: "/settings", label: "Settings" },
];

export function SiteHeader() {
  const { user, loading, signOut } = useAuth();
  const pathname = usePathname();
  const router = useRouter();

  return (
    <header className="border-b">
      <div className="mx-auto flex h-14 w-full max-w-6xl items-center gap-6 px-4">
        <Link href="/" className="font-semibold tracking-tight">
          CSVoyant
        </Link>

        {user && (
          <nav className="flex items-center gap-4 text-sm">
            {links.map((l) => (
              <Link
                key={l.href}
                href={l.href}
                className={cn(
                  "text-muted-foreground transition-colors hover:text-foreground",
                  pathname.startsWith(l.href) && "text-foreground",
                )}
              >
                {l.label}
              </Link>
            ))}
            {/* The admin area is only meaningful for admins. */}
            {user.role === "admin" && (
              <Link
                href="/admin"
                className={cn(
                  "text-muted-foreground transition-colors hover:text-foreground",
                  pathname.startsWith("/admin") && "text-foreground",
                )}
              >
                Admin
              </Link>
            )}
          </nav>
        )}

        <div className="ml-auto flex items-center gap-3">
          {loading ? null : user ? (
            <>
              <span className="hidden text-sm text-muted-foreground sm:inline">{user.email}</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  signOut();
                  router.push("/login");
                }}
              >
                Sign out
              </Button>
            </>
          ) : (
            <>
              <Button variant="ghost" size="sm" render={<Link href="/login">Sign in</Link>} />
              <Button size="sm" render={<Link href="/register">Create account</Link>} />
            </>
          )}
        </div>
      </div>
    </header>
  );
}
