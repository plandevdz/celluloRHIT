<#
.SYNOPSIS
    Génère les scripts Intune qui installent, sur les PC Windows, le certificat racine HTTPS
    de « Celluloplast | Parcours salarié » (autorité interne de Caddy).

.DESCRIPTION
    Le script lit le certificat racine exporté du serveur (celluloplast-root.crt, PEM ou DER),
    vérifie qu'il s'agit bien d'une autorité racine (auto-signée, « CA » = vrai), puis écrit
    dans le dossier de sortie :

      Installer-CertificatCelluloplast.ps1    script de plateforme Intune, ou script de correction
      Detecter-CertificatCelluloplast.ps1     script de détection (Intune « Corrections »)
      Desinstaller-CertificatCelluloplast.ps1 retrait (changement d'autorité, retour arrière)
      empreintes.txt                          empreintes à comparer avec celles du serveur

    Le certificat est embarqué dans les scripts (Intune ne distribue qu'un fichier par script).
    Seule la partie PUBLIQUE est utilisée : la clé privée de l'autorité ne quitte jamais le serveur.
    Les fichiers sont écrits en UTF-8 avec BOM : Windows PowerShell 5.1, utilisé par Intune,
    lit alors correctement les accents.

.PARAMETER Certificat
    Chemin du certificat racine exporté du serveur (celluloplast-root.crt).

.PARAMETER Sortie
    Dossier de sortie (créé si besoin). Par défaut : .\intune-celluloplast

.PARAMETER SansFirefox
    N'ajoute pas la stratégie Firefox « ImportEnterpriseRoots ».

.EXAMPLE
    .\Generer-ScriptsIntune.ps1 -Certificat .\celluloplast-root.crt
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $Certificat,
    [string] $Sortie = (Join-Path (Get-Location) 'intune-celluloplast'),
    [switch] $SansFirefox
)

$ErrorActionPreference = 'Stop'

function ConvertTo-Hex([byte[]] $Octets) { -join ($Octets | ForEach-Object { $_.ToString('X2') }) }

# ------------------------------------------------------------------ lecture et contrôles
$chemin = (Resolve-Path -LiteralPath $Certificat).Path
$texte = [IO.File]::ReadAllText($chemin)
if ($texte -match '-----BEGIN CERTIFICATE-----(?<b64>[\s\S]+?)-----END CERTIFICATE-----') {
    $octets = [Convert]::FromBase64String(($Matches['b64'] -replace '\s', ''))
} else {
    $octets = [IO.File]::ReadAllBytes($chemin)
}
$cert = [Security.Cryptography.X509Certificates.X509Certificate2]::new($octets)

$contraintes = $cert.Extensions | Where-Object { $_ -is [Security.Cryptography.X509Certificates.X509BasicConstraintsExtension] }
if (-not $contraintes -or -not $contraintes.CertificateAuthority) {
    throw "Ce certificat n'est pas une autorité de certification : vous avez sans doute exporté le certificat du site au lieu de root.crt."
}
if ($cert.Subject -ne $cert.Issuer) {
    throw "Ce certificat n'est pas auto-signé (émetteur : $($cert.Issuer)) : il s'agit d'une autorité intermédiaire, pas de la racine. Exportez root.crt."
}
if ($cert.NotAfter -lt (Get-Date)) { throw "Ce certificat a expiré le $($cert.NotAfter)." }
if ($cert.Subject -notlike '*Caddy Local Authority*') {
    Write-Warning "Sujet inattendu : $($cert.Subject). Vérifiez qu'il s'agit bien de l'autorité du serveur Celluloplast."
}

$sha1 = $cert.Thumbprint.ToUpperInvariant()
$sha256 = ConvertTo-Hex ([Security.Cryptography.SHA256]::Create().ComputeHash($cert.RawData))
$sha256Openssl = ($sha256 -split '(..)' | Where-Object { $_ }) -join ':'
$base64 = [Convert]::ToBase64String($cert.RawData)
$fin = $cert.NotAfter.ToString('dd/MM/yyyy')
$date = (Get-Date).ToString('dd/MM/yyyy HH:mm')
$fichier = Split-Path -Leaf $chemin
$firefox = if ($SansFirefox) { '$false' } else { '$true' }

$entete = @'
#  Certificat : __SUJET__
#  Empreinte SHA-1 (Windows) : __SHA1__
#  Empreinte SHA-256         : __SHA256__
#  Valide jusqu'au           : __FIN__
#  Généré le __DATE__ par Generer-ScriptsIntune.ps1 à partir de « __FICHIER__ ».
'@

# ------------------------------------------------------------------ script d'installation
$installer = @'
# Celluloplast | Parcours salarié : installation du certificat racine HTTPS sur ce PC.
__ENTETE__
#
#  À exécuter en SYSTEM, dans l'hôte PowerShell 64 bits (Intune : « Exécuter ce script à l'aide des
#  informations d'identification de l'utilisateur connecté » = Non ; « hôte PowerShell 64 bits » = Oui).
#  Ajoute le certificat dans Ordinateur local > Autorités de certification racines de confiance.
#  Idempotent : ne fait rien si le certificat est déjà présent.
#  Codes de sortie : 0 = installé ou déjà présent ; 1 = échec (voir le journal).
#  Journal : C:\ProgramData\Celluloplast\certificat-racine.log

$ErrorActionPreference = 'Stop'
$EmpreinteAttendue = '__SHA1__'
$CertificatBase64  = '__BASE64__'
$ConfigurerFirefox = __FIREFOX__

$Dossier = Join-Path $env:ProgramData 'Celluloplast'
$Journal = Join-Path $Dossier 'certificat-racine.log'

function Write-Journal([string] $Message) {
    $ligne = '{0:yyyy-MM-dd HH:mm:ss} {1}' -f (Get-Date), $Message
    try { Add-Content -LiteralPath $Journal -Value $ligne -Encoding UTF8 } catch { }
    Write-Output $ligne
}

try {
    if (-not (Test-Path -LiteralPath $Dossier)) { New-Item -ItemType Directory -Path $Dossier -Force | Out-Null }
    $compte = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    Write-Journal "Début : compte $compte, PowerShell $($PSVersionTable.PSVersion), 64 bits = $([Environment]::Is64BitProcess)."

    $cert = [Security.Cryptography.X509Certificates.X509Certificate2]::new([Convert]::FromBase64String($CertificatBase64))
    if ($cert.Thumbprint -ne $EmpreinteAttendue) {
        throw "empreinte inattendue ($($cert.Thumbprint)) : le script a été modifié, régénérez-le."
    }

    if (Test-Path -LiteralPath "Cert:\LocalMachine\Root\$EmpreinteAttendue") {
        Write-Journal "Déjà présent : $($cert.Subject) ($EmpreinteAttendue)."
    } else {
        $magasin = [Security.Cryptography.X509Certificates.X509Store]::new('Root', 'LocalMachine')
        $magasin.Open('ReadWrite')
        try {
            $magasin.Add($cert)
        } finally {
            $magasin.Close()
        }
        Write-Journal "Ajouté : $($cert.Subject) ($EmpreinteAttendue), valide jusqu'au $($cert.NotAfter.ToString('dd/MM/yyyy'))."
    }

    if (-not (Test-Path -LiteralPath "Cert:\LocalMachine\Root\$EmpreinteAttendue")) {
        throw "le certificat est absent du magasin après l'ajout."
    }

    if ($ConfigurerFirefox) {
        # Firefox utilise son propre magasin : cette stratégie lui fait reprendre les racines de Windows.
        $cle = 'HKLM:\SOFTWARE\Policies\Mozilla\Firefox\Certificates'
        if (-not (Test-Path -LiteralPath $cle)) { New-Item -Path $cle -Force | Out-Null }
        New-ItemProperty -Path $cle -Name 'ImportEnterpriseRoots' -PropertyType DWord -Value 1 -Force | Out-Null
        Write-Journal 'Firefox : stratégie ImportEnterpriseRoots = 1.'
    }

    Write-Journal 'Terminé avec succès.'
    exit 0
} catch {
    Write-Journal "ÉCHEC : $($_.Exception.Message)"
    exit 1
}
'@

# ------------------------------------------------------------------ script de détection
$detecter = @'
# Celluloplast | Parcours salarié : détection du certificat racine HTTPS (Intune « Corrections »).
__ENTETE__
#
#  Code 0 = conforme (certificat présent) ; code 1 = non conforme (Intune lance alors le script d'installation).

$EmpreinteAttendue = '__SHA1__'
if (Test-Path -LiteralPath "Cert:\LocalMachine\Root\$EmpreinteAttendue") {
    Write-Output "Conforme : certificat $EmpreinteAttendue présent."
    exit 0
}
Write-Output "Non conforme : certificat $EmpreinteAttendue absent."
exit 1
'@

# ------------------------------------------------------------------ script de retrait
$desinstaller = @'
# Celluloplast | Parcours salarié : retrait du certificat racine HTTPS de ce PC.
__ENTETE__
#
#  À utiliser seulement si l'autorité du serveur a changé (nouvelle installation sans restauration des
#  secrets) ou pour revenir en arrière. À exécuter en SYSTEM, hôte 64 bits.

$ErrorActionPreference = 'Stop'
$EmpreinteAttendue = '__SHA1__'
$Dossier = Join-Path $env:ProgramData 'Celluloplast'
$Journal = Join-Path $Dossier 'certificat-racine.log'
if (-not (Test-Path -LiteralPath $Dossier)) { New-Item -ItemType Directory -Path $Dossier -Force | Out-Null }
try {
    $chemin = "Cert:\LocalMachine\Root\$EmpreinteAttendue"
    if (Test-Path -LiteralPath $chemin) {
        Remove-Item -LiteralPath $chemin
        $message = "Retiré : $EmpreinteAttendue."
    } else {
        $message = "Déjà absent : $EmpreinteAttendue."
    }
    Add-Content -LiteralPath $Journal -Value ('{0:yyyy-MM-dd HH:mm:ss} {1}' -f (Get-Date), $message) -Encoding UTF8
    Write-Output $message
    exit 0
} catch {
    Write-Output "ÉCHEC : $($_.Exception.Message)"
    exit 1
}
'@

# ------------------------------------------------------------------ écriture
function Complete-Modele([string] $Modele) {
    $r = $Modele.Replace('__ENTETE__', $entete.TrimEnd())
    $r = $r.Replace('__SUJET__', $cert.Subject)
    $r = $r.Replace('__SHA1__', $sha1)
    $r = $r.Replace('__SHA256__', $sha256Openssl)
    $r = $r.Replace('__FIN__', $fin)
    $r = $r.Replace('__DATE__', $date)
    $r = $r.Replace('__FICHIER__', $fichier)
    $r = $r.Replace('__BASE64__', $base64)
    $r.Replace('__FIREFOX__', $firefox)
}

if (-not (Test-Path -LiteralPath $Sortie)) { New-Item -ItemType Directory -Path $Sortie -Force | Out-Null }
$sortieComplete = (Resolve-Path -LiteralPath $Sortie).Path
$utf8Bom = New-Object System.Text.UTF8Encoding $true

$fichiers = [ordered]@{
    'Installer-CertificatCelluloplast.ps1'    = $installer
    'Detecter-CertificatCelluloplast.ps1'     = $detecter
    'Desinstaller-CertificatCelluloplast.ps1' = $desinstaller
}
foreach ($nom in $fichiers.Keys) {
    $contenu = (Complete-Modele $fichiers[$nom]) -replace "`r?`n", "`r`n"
    [IO.File]::WriteAllText((Join-Path $sortieComplete $nom), $contenu, $utf8Bom)
}

$empreintes = @"
Certificat racine HTTPS de Celluloplast | Parcours salarié
Sujet           : $($cert.Subject)
Valide du       : $($cert.NotBefore.ToString('dd/MM/yyyy')) au $fin
SHA-1 (Windows) : $sha1
SHA-256         : $sha256Openssl

Sur le serveur, ces deux commandes doivent afficher les mêmes valeurs :
  openssl x509 -noout -fingerprint -sha1   -in /tmp/celluloplast-root.crt
  openssl x509 -noout -fingerprint -sha256 -in /tmp/celluloplast-root.crt
"@
[IO.File]::WriteAllText((Join-Path $sortieComplete 'empreintes.txt'), ($empreintes -replace "`r?`n", "`r`n"), $utf8Bom)

Write-Host ''
Write-Host "Scripts générés dans $sortieComplete" -ForegroundColor Green
Write-Host "  Sujet   : $($cert.Subject)"
Write-Host "  SHA-1   : $sha1"
Write-Host "  SHA-256 : $sha256Openssl"
Write-Host "  Fin     : $fin"
Write-Host ''
Write-Host 'Comparez ces empreintes avec celles affichées par openssl sur le serveur avant de déployer.'
