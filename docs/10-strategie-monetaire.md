# 10 — Stratégie monétaire

> Ce document se fonde sur les hypothèses de `docs/01` §8 (prix,
> conversion) ; il n'en crée aucune nouvelle. Tout chiffre repris ici est
> étiqueté **[HYPOTHÈSE]**, jamais prouvé. Les décisions finales (prix,
> licence produit, dons) appartiennent au commanditaire.

---

## 1. La séparation de départ (ce qui est déjà dans le code)

La ligne gratuit / payant existe depuis P3-P4 et ne bouge pas :

- **Gratuit, complet, local** : dictée, trois moteurs locaux (whisper.cpp,
  Parakeet ONNX, Nemotron streaming), post-traitement local, profil par
  défaut, historique mémoire, fenêtre de secours, pastille d'état.
- **Fonctions Pro nourries à la licence hors ligne** (signature Ed25519,
  grâce hors ligne documentée) :
  - `history-on-disk` — historique chiffré DPAPI plutôt que mémoire seule ;
  - `extra-profiles` — profils multiples par application ;
  - l'option `cloud-post` reste FERMÉE tant que le consentement
    correspondant n'est pas donné. Ce consentement est gratuit : ce n'est
    pas un argument de vente.

Si la ligne de partage devait bouger, c'est une décision produit — le code
ne l'inventera pas.

## 2. Formes d'offre possibles [HYPOTHÈSE à valider]

Trois formes restent compatibles avec le produit sans réécriture :

| Forme | Contenu | Risque principal |
|---|---|---|
| Licence unique « Voxli Pro » (définitive) | les fonctions Pro ci-dessus | conversion faible selon le public cible — à mesurer avant de figer un prix |
| Abonnement annuel | idem + support | coût du support jamais estimé |
| Gratuit + dons | tout gratuit | les coûts du projet doivent être couverts ailleurs — voir §5 |

Le prix exact reste une décision du commanditaire. Les hypothèses chiffrées
déjà posées dans docs/01 §8 ne sont ni reprises ni retranchées ici pour ne
pas créer de double source — docs/01 fait foi.

## 3. Ce qui se vend, ce qui ne se vend pas

- Ne jamais vendre la vie privée « à sens unique » : le mode local strict
  gratuit EST la promesse du produit ; le payant ajoute du CONFORT
  (historique durable, profils multiples), pas des permissions nouvelles
  d'envoi réseau.
- La transcription distante BYOK (P4) n'est pas une fonction Pro : elle
  requiert une clé que l'utilisateur a payée AU FOURNISSEUR tier — le
  produit se contente de l'acheminer, sans marge, en cohérence avec ADR-07.
- Aucun modèle de reconnaissance n'est revendu : leurs licences
  (MIT, CC-BY-4.0, OpenMDW) imposent l'attribution et la gratuité du
  MODÈLE. Ce que la licence produit couvre, c'est le CODE.

## 4. Licence du code produit — décision n°4 en attente

Le dépôt reste « licence à fixer avant toute distribution » (README §7,
décision n°4 de docs/01). Conséquence directe : aucune distribution
publique tant que cette décision humaine n'est pas prise. Rien de neuf
n'est inventé ici : les options et conséquences sont dans docs/01.

## 5. Dons et FUNDING.yml — conditions strictes

Le fichier `.github/FUNDING.yml` n'existe pas au jour de la rédaction et
n'existera pas tant que :

1. le commanditaire a vérifié les destinations possibles (seulement des
   plateformes où le compte existe réellement et appartient au projet) ;
2. la licence produit est décidée (§4) — la cohérence « gratuit + dons »
   contre « payant » doit être tranchée AVANT d'afficher une voie de
   collecte d'argent.

Aucun lien de don dans l'interface ne précède ces deux conditions.

## 6. Mesures d'apprentissage avant de fixer le prix

- [À FAIRE] Enquête d'usage : le nombre de dictées par utilisateur actif
  est déjà observable en local (historique mémoire, RF-33). La télémétrie a
  été REFUSÉE (telemetry=off par défaut, RF-35) ; l'apprentissage passe
  donc par l'examen de l'usage par l'utilisateur lui-même (données
  exportables, jamais sorties).
- [RECO] bêta ouverte d'observation, avec consultation volontaire des
  utilisateurs, avant de trancher prix ou abonnement.

## 7. Rappel de cohérence

Ce document s'attache à docs/01 §8 sous cette réserve : en cas d'écart,
docs/01 fait foi (document supérieur, plus ancien, revu par le
commanditaire). Ce n'est pas le rôle de docs/10 de redéfinir les
hypothèses : seulement de les porter jusqu'à l'application produit.
