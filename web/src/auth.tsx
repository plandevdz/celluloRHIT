import { createContext, ReactNode, useCallback, useContext, useEffect, useState } from "react";
import { api, ApiError, SESSION_EXPIRED_EVENT } from "./api";
import type { Role } from "./labels";

export type Me = { id: string; email: string; display_name: string; roles: Role[] };

type AuthState = {
  user: Me | null;
  loading: boolean;
  expired: boolean;
  has: (...roles: Role[]) => boolean;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
};

const AuthContext = createContext<AuthState | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<Me | null>(null);
  const [loading, setLoading] = useState(true);
  const [expired, setExpired] = useState(false);

  useEffect(() => {
    api<Me>("/auth/me")
      .then(setUser)
      .catch(() => setUser(null))
      .finally(() => setLoading(false));
    // Session expirée en cours de travail : on garde la page (et la saisie)
    // et on demande une reconnexion par-dessus.
    const onExpired = () => setExpired(true);
    window.addEventListener(SESSION_EXPIRED_EVENT, onExpired);
    return () => window.removeEventListener(SESSION_EXPIRED_EVENT, onExpired);
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    const me = await api<Me>("/auth/login", { body: { email, password } });
    setUser(me);
    setExpired(false);
  }, []);

  const logout = useCallback(async () => {
    try {
      await api("/auth/logout", { body: {} });
    } finally {
      setUser(null);
      setExpired(false);
    }
  }, []);

  const has = useCallback((...roles: Role[]) => !!user && roles.some((r) => user.roles.includes(r)), [user]);

  return (
    <AuthContext.Provider value={{ user, loading, expired, has, login, logout }}>{children}</AuthContext.Provider>
  );
}

export function useAuth(): AuthState {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("AuthProvider manquant");
  return ctx;
}

export function loginErrorMessage(e: unknown): string {
  if (e instanceof ApiError) return e.message;
  return "Connexion impossible";
}
