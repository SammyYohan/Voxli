# Security policy — Voxli

La version française fait foi (langue du dépôt) ; l'anglais résume ci-dessous.

## Signaler une faille

- **Canal privé** : utilise le signalement privé de GitHub
  (« Security advisories → Report a vulnerability ») si activé sur ce
  dépôt ; sinon, ouvre une **issue privée** en contactant le mainteneur
  directement. *Coordonnées à compléter par le mainteneur : aucune adresse
  e-mail n'est publiée dans ce fichier tant qu'elle n'est pas vérifiée. —
  [À REMPLIR]*.
- **Ne pas publier** la faille dans une issue publique avant coordination.
- Réponse attendue : aucune prestation de délai n'est promise au jour de
  la rédaction [RECO : viser une accusé de réception sous 7 jours].

## Périmètre

- Application : `apps/desktop` (Tauri 2 + WebView), crates `voxli-*`.
- Les points de vigilance documentés : extraction d'archives de modèles
  (path traversal, corrigé en audit P1-5 avec validation avant/après),
  empreintes de confiance des modèles (P1-3), licence Ed25519 hors ligne,
  fenêtre de secours (seule surface exposant du texte transcrit),
  insertion système (SendInput) et lecture de contexte (RF-37, opt-in).

## Abord phasé

- Zéro récompense (bug bounty) : le projet ne promet pas de bourse tant
  que la monétisation (`docs/10`) n'est pas tranchée.
- Droits : tout correctif de sécurité suit la checklist `docs/11` et le
  changelog ; les issues corrigées portent la référence au code.

## Rappels de conception sécurité (immuables)

- Le produit n'embarque AUCUN client réseau pendant l'usage standard ;
  les connexions possibles sont listées dans `docs/vie-privee.md` §5.
- L'API d'insertion agit UNIQUEMENT sur un geste utilisateur (RF-22) et
  refuse les champs sensibles (listes d'exclusion + détection mot de passe).
- Aucun secret utilisateur n'est stocké dans `config.json` (ADR-07) ; la
  clé API du moteur distant, si saisie, vit dans `voxli-secure`.

---

## English summary

Report vulnerabilities privately (GitHub private vulnerability reporting if
available, otherwise the maintainer — contact placeholder above). Do not
open public issues for unpatched vulnerabilities. Scope: the Tauri desktop
app and the `voxli-*` crates. No bug bounty until monetisation decisions
are made (see docs/10). Known-by-design constraints and the exact list of
possible network egress are documented in docs/vie-privee.md §5.
No published e-mail address — nothing invented here.
