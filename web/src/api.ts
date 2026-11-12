// Client de l'API /api/v1. Aucun jeton côté navigateur : la session est un
// cookie HttpOnly ; chaque commande porte l'en-tête x-cellulo-client.

export class ApiError extends Error {
  status: number;
  code: string;
  field?: string;
  detail?: any;
  constructor(status: number, code: string, message: string, field?: string, detail?: any) {
    super(message);
    this.status = status;
    this.code = code;
    this.field = field;
    this.detail = detail;
  }
}

export const SESSION_EXPIRED_EVENT = "cellulo:session-expired";

type Options = {
  method?: string;
  body?: unknown;
  idempotencyKey?: string;
};

export async function api<T = any>(path: string, opts: Options = {}): Promise<T> {
  const headers: Record<string, string> = { "x-cellulo-client": "web" };
  if (opts.body !== undefined) headers["content-type"] = "application/json";
  if (opts.idempotencyKey) headers["idempotency-key"] = opts.idempotencyKey;
  let resp: Response;
  try {
    resp = await fetch(`/api/v1${path}`, {
      method: opts.method ?? (opts.body !== undefined ? "POST" : "GET"),
      headers,
      credentials: "same-origin",
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    });
  } catch {
    throw new ApiError(
      0,
      "reseau",
      "Serveur injoignable. Votre saisie est conservée : vérifiez le réseau puis réessayez.",
    );
  }
  return handle<T>(resp, path);
}

export async function upload<T = any>(path: string, file: File): Promise<T> {
  const form = new FormData();
  form.append("file", file);
  let resp: Response;
  try {
    resp = await fetch(`/api/v1${path}`, {
      method: "POST",
      headers: { "x-cellulo-client": "web" },
      credentials: "same-origin",
      body: form,
    });
  } catch {
    throw new ApiError(0, "reseau", "Serveur injoignable : le fichier n'a pas été déposé.");
  }
  return handle<T>(resp, path);
}

async function handle<T>(resp: Response, path: string): Promise<T> {
  const text = await resp.text();
  let data: any = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  if (!resp.ok) {
    if (resp.status === 401 && !path.startsWith("/auth/")) {
      window.dispatchEvent(new Event(SESSION_EXPIRED_EVENT));
    }
    throw new ApiError(
      resp.status,
      data?.error ?? "erreur",
      data?.message ?? `Erreur ${resp.status}`,
      data?.field,
      data?.detail,
    );
  }
  return data as T;
}

/** Clé d'idempotence stable pour un formulaire : un nouvel essai après une
 *  coupure réseau ne crée pas de doublon. */
export function newKey(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
