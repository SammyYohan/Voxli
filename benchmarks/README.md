# Bancs de mesures — Voxli

Deux bancs seulement, en P0, avec un cadre reproductible (docs/06 §4). Les
résultats vont dans `benchmarks/resultats-<date>.md` (non suivi).

## M1 — Latence bout en bout

Cible P0 : **p95 ≤ 1,0 s pour 10 s d'audio** (moteur réel). Le moteur factice
sert de référence pure logique (l'inférence n'est pas mesurée là).

Ce que mesure le produit déjà, via `voxli-diagnostics` (aucune instrumentation
supplémentaire) :

| Étape | Champ journal | Champ utile au calcul |
|---|---|---|
| détection relâchement → début pipeline | `state://changed` recording → finalizing | t0 |
| analyse VAD | `Latency { stage: "vad", micros }` | — |
| inférence | `Latency { stage: "inference", micros }` | — |
| post-traitement | `Latency { stage: "post-process", micros }` | — |
| insertion | `Latency { stage: "insert", micros }` | — |

Protocole :
1. lancer Voxli compilé en `--release` (profil `lto = "thin"`) ;
2. dicter 3 séries de 10 phrases calmes de ~10 s (fixtures `long-10s.wav`
   pour la répétabilité hors micro, le micro pour la représentativité) ;
3. extraire les micros par étape : `grep '"event":"latency"' voxli.log`
   et calculer p50/p95 par étape et cumulés (t0 → SessionFinished).

## M2 — Mémoire

Cible : repos < 210 Mo (RF-07, ADR-09) ; usage : dérive lente max +15 %.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/measure-memory.ps1 -Seconds 120
```

Pendant la fenêtre : une dictée courte (hors inférence réelle) et une longue
(10 s), puis 1 min de repos. Relever min/moy/p95/max au tableau, comparer à la
référence P0.

## Hypothèses à instrumenter (mesurées par ces bancs)

- **H3** (fiabilité d'insertion, retient les utilisateurs) — le taux de repli
  se lit dans le journal : répartition des `InsertResult` par
  `fallback_reason`, cible **< 2 %** de replis (niveau 3/4) sur les 12
  applications de `tests/e2e/matrice-12-apps.md`.
- **H6** (seuil de latence perçue < ~2 s) — les cumuls p95 du banc M1
  n'atteignent pas le seuil : M1 valide l'exigence produit (≤ 1,0 s pour
  10 s d'audio), largement sous le seuil perceptuel.
- **Confort des longues dictées** (dérive mémoire) — banc M2 : clichés à T0,
  T+30 min, T+2 h ; dérive max +15 %.
