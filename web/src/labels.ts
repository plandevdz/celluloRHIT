// Libellés français et formats d'affichage.

export type Role = "ADMIN" | "IT" | "RH" | "GRAPHISTE" | "FORMATION";

export const ROLES: Role[] = ["ADMIN", "RH", "IT", "FORMATION", "GRAPHISTE"];

export const roleLabel: Record<string, string> = {
  ADMIN: "Administration",
  RH: "RH",
  IT: "IT",
  FORMATION: "Formation",
  GRAPHISTE: "Graphiste",
};

export const NEED_CATEGORIES = [
  "pc",
  "telephone",
  "sim",
  "email_pro",
  "xena",
  "intelix",
  "carte_visite",
  "vehicule",
  "formation",
] as const;
export type NeedCategory = (typeof NEED_CATEGORIES)[number];

export const needLabel: Record<string, string> = {
  pc: "PC",
  telephone: "Téléphone",
  sim: "SIM / ligne",
  email_pro: "Adresse email professionnelle",
  xena: "Accès Xena",
  intelix: "Accès Intelix",
  carte_visite: "Cartes de visite",
  vehicule: "Véhicule",
  formation: "Formations",
};

export const needTeam: Record<string, Role> = {
  pc: "IT",
  telephone: "IT",
  sim: "IT",
  email_pro: "IT",
  xena: "IT",
  intelix: "IT",
  carte_visite: "GRAPHISTE",
  vehicule: "RH",
  formation: "FORMATION",
};

export const answerLabel: Record<string, string> = {
  oui: "Oui",
  non: "Non",
  a_preciser: "À préciser",
};

export const preferenceLabel: Record<string, string> = {
  fixe: "PC fixe de préférence",
  portable: "Portable de préférence",
  sans_preference: "Sans préférence",
};

export const caseKindLabel: Record<string, string> = {
  ONB: "Arrivée",
  MUT: "Mutation",
  OFF: "Départ",
  DEM: "Demande ponctuelle",
};

export const caseStatusLabel: Record<string, string> = {
  ouvert: "Ouvert",
  clos: "Clos",
  annule: "Annulé",
};

export const taskStatusLabel: Record<string, string> = {
  a_faire: "À prendre",
  en_cours: "En cours",
  bloque: "En attente",
  termine: "Terminée",
  annule: "Annulée",
};

export const employeeStatusLabel: Record<string, string> = {
  a_venir: "À venir",
  actif: "Actif",
  sorti: "Sorti",
};

export const assetStatusLabel: Record<string, string> = {
  disponible: "Disponible",
  reserve: "Réservé",
  affecte: "Remis",
  controle: "En contrôle",
  reparation: "En réparation",
  reforme: "Réformé",
};

export const assetKindLabel: Record<string, string> = {
  pc_portable: "PC portable",
  pc_fixe: "PC fixe",
  telephone: "Téléphone",
  ecran: "Écran",
  imprimante: "Imprimante",
  autre: "Autre",
};

export const sessionStatusLabel: Record<string, string> = {
  a_planifier: "À planifier",
  planifiee: "Planifiée",
  reportee: "Reportée",
  realisee: "Réalisée",
  dispensee: "Dispensée",
  annulee: "Annulée",
};

export const documentStatusLabel: Record<string, string> = {
  a_generer: "PDF en préparation",
  genere: "Imprimable — signature à recueillir",
  signe: "Signé (scan déposé)",
};

export const deliveryStatusLabel: Record<string, string> = {
  en_attente: "En attente",
  journalise: "Consigné — mode test, aucun e-mail envoyé",
  accepte: "Accepté par Exchange",
  a_verifier: "À vérifier",
  echec: "En erreur",
};

export const resolutionLabel: Record<string, string> = {
  remis: "Remis",
  deja_detenu: "Déjà détenu",
  exception: "Exception motivée",
  realise: "Réalisé",
};

/** Phrase exacte sur la diffusion d'un avis, selon le mode d'envoi. */
export function visibilityText(roles: string[], mailMode: string): string {
  const teams = roles.map((r) => roleLabel[r] ?? r).join(", ");
  return mailMode === "journal"
    ? `Dossier visible par ${teams} — aucun e-mail envoyé (mode test)`
    : `Dossier visible par ${teams} — avis par e-mail en file d'envoi`;
}

const TZ = "Africa/Algiers";

/** Date RH (AAAA-MM-JJ) → JJ/MM/AAAA, sans conversion de fuseau. */
export function fmtDate(d?: string | null): string {
  if (!d) return "—";
  const [y, m, day] = d.slice(0, 10).split("-");
  return `${day}/${m}/${y}`;
}

/** Instant UTC → date et heure d'Alger. */
export function fmtDateTime(iso?: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("fr-FR", {
    timeZone: TZ,
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function todayIso(): string {
  return new Date().toLocaleDateString("sv-SE", { timeZone: TZ });
}
