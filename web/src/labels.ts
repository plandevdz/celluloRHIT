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
  en_service: "En service",
  controle: "En contrôle",
  reparation: "En réparation",
  reforme: "Réformé",
};

export const assetKindLabel: Record<string, string> = {
  pc_portable: "PC portable",
  pc_fixe: "PC fixe",
  telephone: "Téléphone",
  tablette: "Tablette",
  ecran: "Écran",
  imprimante: "Imprimante",
  scanner: "Scanner",
  serveur: "Serveur",
  onduleur: "Onduleur",
  reseau: "Réseau",
  pointeuse: "Pointeuse",
  videoprojecteur: "Vidéoprojecteur",
  destructeur: "Destructeur",
  videosurveillance: "Vidéosurveillance",
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
    : `Dossier visible par ${teams} — avis par e-mail en file d'envoi (Exchange)`;
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

// ------------------------------------------------------------ lot 3 : parc, véhicules, transitions

export const assetConditionLabel: Record<string, string> = {
  neuf: "Neuf",
  bon: "Bon",
  use: "Usé",
  hs: "Hors service",
};

export const contractTypeLabel: Record<string, string> = {
  cdi: "CDI",
  cdd: "CDD",
  stage: "Stage",
  interim: "Intérim",
  apprentissage: "Apprentissage",
  consultant: "Consultant",
};

export const DEPARTMENTS = [
  "Direction",
  "Commercial",
  "Marketing",
  "Finance",
  "Juridique",
  "RH",
  "IT",
  "Logistique",
  "Moyens généraux",
  "Qualité",
  "Production",
  "SAV Audiologie",
];

export const lineTypeLabel: Record<string, string> = { voix: "Voix", data: "Data", voix_data: "Voix + Data" };
export const lineStatusLabel: Record<string, string> = { active: "Active", suspendue: "Suspendue", resiliee: "Résiliée" };

export const vehicleKindLabel: Record<string, string> = { propriete: "Propriété", location: "Location" };
export const vehicleStatusLabel: Record<string, string> = {
  disponible: "Disponible",
  affecte: "Affecté",
  immobilise: "Immobilisé",
  rendu_loueur: "Rendu au loueur",
  reforme: "Réformé",
};
export const vehicleDocLabel: Record<string, string> = {
  assurance: "Assurance",
  controle_technique: "Contrôle technique",
  carte_grise: "Carte grise",
  vignette: "Vignette",
  autre: "Autre pièce",
};
export const maintenanceLabel: Record<string, string> = {
  vidange: "Vidange",
  entretien: "Entretien",
  reparation: "Réparation",
  pneus: "Pneus",
  autre: "Autre",
};
export const vehicleMovementLabel: Record<string, string> = {
  remise: "Remise",
  restitution: "Restitution",
  retour_loueur: "Retour au loueur",
  reprise: "Reprise SharePoint",
};
export const leaseStatusLabel: Record<string, string> = { en_cours: "En cours", renouvele: "Renouvelé", restitue: "Restitué au loueur" };

export const licenseKindLabel: Record<string, string> = { abonnement: "Abonnement", perpetuelle: "Licence perpétuelle" };
export const seatReleaseLabel: Record<string, string> = {
  immediat: "Libéré immédiatement",
  fin_periode: "Libéré en fin de période",
  non_reattribuable: "Non réattribuable",
};

export const scanResultLabel: Record<string, string> = {
  vu: "Vu au bon endroit",
  ailleurs: "Vu ailleurs",
  hors_perimetre: "Hors périmètre",
  inconnu: "Code inconnu",
  doublon: "Déjà pointé ici",
};

export const transferStatusLabel: Record<string, string> = {
  brouillon: "Changement en préparation",
  confirmee: "Changement planifié",
  appliquee: "Poste appliqué",
  suspendue: "Application suspendue",
  annulee: "Annulée",
};
export const deltaActionLabel: Record<string, string> = {
  conserver: "Conserver",
  ajouter: "Ajouter",
  reprendre: "Reprendre",
  verifier: "Vérifier",
};
export const deltaKindLabel: Record<string, string> = {
  compte: "Accès",
  licence: "Licence",
  bien: "Bien",
  vehicule: "Véhicule",
  besoin: "Besoin du profil",
  formation: "Formation",
};

export const departureReasonLabel: Record<string, string> = {
  demission: "Démission",
  fin_contrat: "Fin de contrat",
  rupture: "Rupture",
  retraite: "Retraite",
  autre: "Autre",
};
export const departureStatusLabel: Record<string, string> = {
  planifie: "Départ planifié",
  sortie_effective: "Sortie effective",
  annule: "Départ annulé",
};
export const exitStateLabel: Record<string, string> = {
  a_traiter: "À traiter",
  fait: "Fait",
  ecart: "Écart signalé",
  perte: "Perte décidée",
  non_applicable: "Sans objet",
};

export const archiveStatusLabel: Record<string, string> = {
  desactive: "Copie locale (archivage désactivé)",
  en_attente: "Archivage en attente",
  archive: "Archivé sur SharePoint",
  echec: "Archivage en échec",
};

export const repriseProposalLabel: Record<string, string> = {
  creation: "Création",
  mise_a_jour: "Mise à jour",
  inchange: "Inchangée",
  a_revoir: "À revoir",
  rejet: "Rejetée",
};
export const repriseResultLabel: Record<string, string> = {
  cree: "Créée",
  mis_a_jour: "Mise à jour",
  inchange: "Inchangée",
  ignore: "Ignorée",
  conflit: "En conflit",
  rejete: "Rejetée",
};

/** Instant UTC → valeur d'un champ datetime-local en heure d'Alger. */
export function toLocalInput(iso?: string | null): string {
  if (!iso) return "";
  const d = new Date(new Date(iso).getTime() + 3600 * 1000);
  return d.toISOString().slice(0, 16);
}

/** Valeur d'un champ datetime-local (heure d'Alger, UTC+1) → ISO UTC. */
export function fromLocalInput(v: string): string | null {
  if (!v) return null;
  return new Date(`${v}:00+01:00`).toISOString();
}
