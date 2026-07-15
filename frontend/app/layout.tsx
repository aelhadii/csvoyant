import type { Metadata } from "next";
import type { ReactNode } from "react";

import { SiteHeader } from "@/components/site-header";
import { Toaster } from "@/components/ui/sonner";
import { AuthProvider } from "@/lib/auth";
import { cn } from "@/lib/utils";
import "./globals.css";

export const metadata: Metadata = {
  title: "CSVoyant",
  description: "Turn a CSV URL into an interactive dashboard",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className={cn("font-sans")}>
      <body className="min-h-screen bg-background text-foreground antialiased">
        <AuthProvider>
          <SiteHeader />
          <main className="mx-auto w-full max-w-6xl px-4 py-8">{children}</main>
          <Toaster richColors />
        </AuthProvider>
      </body>
    </html>
  );
}
