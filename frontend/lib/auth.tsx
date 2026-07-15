"use client";

/**
 * Auth state for the app: the access token lives in memory (never localStorage), and the session
 * is restored on load by exchanging the httpOnly refresh cookie for a fresh token.
 */

import { useRouter } from "next/navigation";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import * as apiClient from "@/lib/api";
import { setAccessToken, type User } from "@/lib/api";

type AuthContextValue = {
  user: User | null;
  /** True until the initial session restore finishes — guards against redirect flicker. */
  loading: boolean;
  signIn: (email: string, password: string) => Promise<void>;
  signUp: (email: string, password: string) => Promise<void>;
  signOut: () => Promise<void>;
  setUser: (u: User | null) => void;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  // Restore the session once on mount: refresh cookie → access token → profile.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (await apiClient.refresh()) {
        try {
          const profile = await apiClient.me();
          if (!cancelled) setUser(profile);
        } catch {
          /* token was rejected — stay signed out */
        }
      }
      if (!cancelled) setLoading(false);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const signIn = useCallback(async (email: string, password: string) => {
    const tokens = await apiClient.login(email, password);
    setAccessToken(tokens.access_token);
    setUser(await apiClient.me());
  }, []);

  const signUp = useCallback(async (email: string, password: string) => {
    const tokens = await apiClient.register(email, password);
    setAccessToken(tokens.access_token);
    setUser(await apiClient.me());
  }, []);

  const signOut = useCallback(async () => {
    // Must revoke server-side: clearing only the in-memory token would leave the refresh cookie
    // valid, and the next page load would silently restore the session.
    await apiClient.logout();
    setUser(null);
  }, []);

  const value = useMemo(
    () => ({ user, loading, signIn, signUp, signOut, setUser }),
    [user, loading, signIn, signUp, signOut],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}

/** Redirect to /login once we know there's no session. */
export function useRequireAuth() {
  const { user, loading } = useAuth();
  const router = useRouter();
  useEffect(() => {
    if (!loading && !user) router.replace("/login");
  }, [loading, user, router]);
  return { user, loading };
}
