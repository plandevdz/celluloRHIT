import { FormEvent, useEffect, useRef, useState } from "react";
import { Link, Navigate, NavLink, Route, Routes, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { api } from "./api";
import { loginErrorMessage, useAuth } from "./auth";
import { Field, Loading, Modal, Notice } from "./components";
import { Role, roleLabel } from "./labels";
import AdminPage from "./pages/AdminPage";
import ArrivalPage from "./pages/ArrivalPage";
import AssetPage from "./pages/AssetPage";
import AssetsPage from "./pages/AssetsPage";
import CasePage from "./pages/CasePage";
import CasesPage from "./pages/CasesPage";
import EmployeePage from "./pages/EmployeePage";
import EmployeesPage from "./pages/EmployeesPage";
import HandoverPage from "./pages/HandoverPage";
import HomePage from "./pages/HomePage";
import ReturnPage from "./pages/ReturnPage";
import TrainingsPage from "./pages/TrainingsPage";
import WorkPage from "./pages/WorkPage";

type NavItem = { to: string; label: string; roles?: Role[]; end?: boolean };

// Seuls les espaces autorisés sont affichés (l'API contrôle de toute façon).
const NAV: NavItem[] = [
  { to: "/", label: "Accueil RH", roles: ["RH", "ADMIN"], end: true },
  { to: "/travail", label: "Mon travail" },
  { to: "/dossiers", label: "Dossiers" },
  { to: "/personnel", label: "Personnel", roles: ["RH", "ADMIN", "IT"] },
  { to: "/parc", label: "Parc IT", roles: ["IT", "ADMIN"] },
  { to: "/formations", label: "Formations", roles: ["FORMATION", "RH", "ADMIN"] },
  { to: "/administration", label: "Administration", roles: ["ADMIN"] },
];

function useVersionWatch() {
  const initial = useRef<string | null>(null);
  const [newVersion, setNewVersion] = useState(false);
  useEffect(() => {
    const check = async () => {
      try {
        const h = await api<{ version: string }>("/health");
        if (initial.current === null) initial.current = h.version;
        else if (h.version !== initial.current) setNewVersion(true);
      } catch {
        /* réseau : on réessaiera */
      }
    };
    check();
    const t = window.setInterval(check, 5 * 60 * 1000);
    return () => window.clearInterval(t);
  }, []);
  return newVersion;
}

export default function App() {
  const { user, loading, expired } = useAuth();
  const location = useLocation();
  const newVersion = useVersionWatch();

  if (location.pathname === "/activation") return <ActivationPage />;
  if (loading) return <Loading />;
  if (!user) return <LoginPage />;

  return (
    <>
      <TopBar />
      {newVersion && (
        <div className="banner" role="status">
          Une nouvelle version de l'application est disponible.
          <button className="btn btn-small" onClick={() => window.location.reload()}>
            Recharger la page
          </button>
          <span className="small">Terminez d'abord la saisie en cours.</span>
        </div>
      )}
      <main>
        <Routes>
          <Route path="/" element={<HomeRoute />} />
          <Route path="/travail" element={<WorkPage />} />
          <Route path="/arrivees/nouvelle" element={<ArrivalPage />} />
          <Route path="/dossiers" element={<CasesPage />} />
          <Route path="/dossiers/:reference" element={<CasePage />} />
          <Route path="/personnel" element={<EmployeesPage />} />
          <Route path="/personnel/:id" element={<EmployeePage />} />
          <Route path="/parc" element={<AssetsPage />} />
          <Route path="/parc/actifs/:id" element={<AssetPage />} />
          <Route path="/parc/remise" element={<HandoverPage />} />
          <Route path="/parc/restitution" element={<ReturnPage />} />
          <Route path="/formations" element={<TrainingsPage />} />
          <Route path="/administration" element={<AdminPage />} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </main>
      {expired && <SessionDialog />}
    </>
  );
}

function HomeRoute() {
  const { has } = useAuth();
  return has("RH", "ADMIN") ? <HomePage /> : <Navigate to="/travail" replace />;
}

function TopBar() {
  const { user, has, logout } = useAuth();
  const navigate = useNavigate();
  if (!user) return null;
  return (
    <header className="topbar">
      <Link to="/" className="brand">
        Celluloplast<small>Parcours salarié</small>
      </Link>
      <nav className="nav" aria-label="Navigation principale">
        {NAV.filter((n) => !n.roles || has(...n.roles)).map((n) => (
          <NavLink key={n.to} to={n.to} end={n.end}>
            {n.label}
          </NavLink>
        ))}
      </nav>
      <div className="userbox">
        <span>
          {user.display_name}
          <span className="roles"> · {user.roles.map((r) => roleLabel[r]).join(" + ")}</span>
        </span>
        <button
          className="btn btn-small"
          onClick={async () => {
            await logout();
            navigate("/");
          }}
        >
          Déconnexion
        </button>
      </div>
    </header>
  );
}

function LoginForm({ onDone, compact }: { onDone?: () => void; compact?: boolean }) {
  const { login, user } = useAuth();
  const [email, setEmail] = useState(user?.email ?? "");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await login(email, password);
      onDone?.();
    } catch (err) {
      setError(loginErrorMessage(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <form onSubmit={submit}>
      <Field label="Adresse email" required>
        <input type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} required autoFocus={!compact} />
      </Field>
      <Field label="Mot de passe" required>
        <input type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} required />
      </Field>
      {error && <Notice kind="danger">{error}</Notice>}
      <button className="btn btn-primary" type="submit" disabled={busy}>
        {busy ? "Connexion…" : "Se connecter"}
      </button>
    </form>
  );
}

function LoginPage() {
  return (
    <div className="login-page">
      <div className="login-card">
        <h1>Celluloplast</h1>
        <p className="subtitle" style={{ marginBottom: "1.2rem" }}>
          Parcours salarié — arrivées, parc IT, formations
        </p>
        <LoginForm />
        <p className="small muted" style={{ marginTop: "1rem" }}>
          Compte nominatif créé par l'administrateur. Mot de passe oublié : contactez l'administration.
        </p>
      </div>
    </div>
  );
}

/** Session expirée : reconnexion par-dessus la page, la saisie reste en place. */
function SessionDialog() {
  return (
    <Modal title="Session expirée" onClose={() => undefined}>
      <Notice kind="warn">Votre session a expiré. Reconnectez-vous : la page et votre saisie sont conservées.</Notice>
      <LoginForm compact />
    </Modal>
  );
}

function ActivationPage() {
  const [params] = useSearchParams();
  const token = params.get("token") ?? "";
  const [pw, setPw] = useState("");
  const [pw2, setPw2] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pw !== pw2) {
      setError("Les deux mots de passe diffèrent.");
      return;
    }
    try {
      await api("/auth/activate", { body: { token, password: pw } });
      setDone(true);
    } catch (err) {
      setError(loginErrorMessage(err));
    }
  };
  return (
    <div className="login-page">
      <div className="login-card">
        <h1>Activation du compte</h1>
        {done ? (
          <>
            <Notice kind="ok">Mot de passe enregistré. Vous pouvez vous connecter.</Notice>
            <a className="btn btn-primary" href="/">
              Aller à la connexion
            </a>
          </>
        ) : (
          <form onSubmit={submit}>
            <p className="subtitle">Choisissez votre mot de passe (10 caractères minimum). Ce lien ne sert qu'une fois.</p>
            <Field label="Nouveau mot de passe" required>
              <input type="password" autoComplete="new-password" minLength={10} value={pw} onChange={(e) => setPw(e.target.value)} required />
            </Field>
            <Field label="Confirmation" required>
              <input type="password" autoComplete="new-password" minLength={10} value={pw2} onChange={(e) => setPw2(e.target.value)} required />
            </Field>
            {error && <Notice kind="danger">{error}</Notice>}
            <button className="btn btn-primary" type="submit" disabled={!token}>
              Activer mon compte
            </button>
            {!token && <Notice kind="danger">Lien incomplet : demandez un nouveau lien à l'administrateur.</Notice>}
          </form>
        )}
      </div>
    </div>
  );
}

function NotFound() {
  return (
    <div className="card">
      <h1>Page introuvable</h1>
      <p>
        <Link to="/">Retour à l'accueil</Link>
      </p>
    </div>
  );
}
