# Contribution à Voxli

Merci de passer du temps sur ce projet. Règles simples, écrites pour
durer — parce que le secret d'un dépôt sain est de ne jamais avoir à se
mentir sur ce qui marche.

## 1. Ce que ce projet refuse (avant tout le reste)

1. **Ne jamais déclarer « vert » sans exécution.** Un test que tu n'as pas
   lancé s'appelle « écrit ». Le libellé [écrit, non compilé] est la
   convention du dépôt — reprends-la tes messages de commit et PR.
2. **Ne jamais inventer de valeurs de confiance** : digests SHA-256, prix,
   licences, e-mails de contact, URL de dons. Un champ incertain vaut
   `None`/vide avec un [À FAIRE], pas une valeur plausible (cf. audit P1-3,
   docs/11 §3).
3. **Ne jamais modifier la licence du produit** sans décision du
   commanditaire (décision n°4, toujours en attente).
4. **Ne jamais casser les garde-fous de vie privée** : télémétrie absente,
   mode local strict par défaut, refus utilisateur explicite avant tout
   réseau (RF-35). Un changement « utile » qui affaiblit la confiance n'est
   pas un changement.

## 2. Comment contribuer en pratique

1. Ouvrir une issue d'abord (le travail sur branche sans discussion est le
   meilleur moyen d'écrire du code jeté) ; une issue bien écrite cite
   les fichiers/ranges.
2. Le compilateur et les tests se lancent AVANT la PR :

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace --all-features
   pnpm --dir apps/desktop lint && pnpm --dir apps/desktop build
   ```

   Les crates cibles matérielles (`voxli-win`, features `real-asr`,
   `nemo-stream`, `history-disk`) exigent Windows + microphone — ce qui
   passe sous Linux est précisé dans `docs/11` §5 et la CI.
3. Convention de commits : préfixe du domaine touché
   (`core:`, `asr:`, `config:`, `ui:`, `docs:`, `ci:`), en français
   toléré, Valeurs passées relatives au journal `CHANGELOG.md` [Non
   publié].

## 3. Style du code

- Rust : doc-commentaire en français, justifications d'`unsafe`
  OBLIGATOIRES (convention ADR-06), jamais de `unwrap()` sur les chemins
  applicatifs (ADR-05 ; toute panique du chemin de tâche utilise
  `catch_unwind` + retour explicite).
- TypeScript : jamais de contenu utilisateur dans `innerHTML` — la
  table i18n est la seule chaîne innerHTML-able (commentaire
  d'`i18n.ts`).
- Docs : les nouveaux documents suivent la numérotation `docs/NN-…` ;
  toute exigence cross-référence ses RF-NN existantes, n'invente pas de
  nouveaux référentiels.

## 4. Traductions (i18n)

- Ajouter une langue = ajouter les clés manquantes de
  `apps/desktop/src/i18n.ts` + une option de `#gen-ui-language`.
- Une clé manquante est VISIBLE (elle s'affiche telle quelle) — c'est le
  mécanisme assumé ; ne cache jamais un trou de traduction au nom d'une
   érgonomie.
- La langue d'écoute ASR n'est PAS une langue d'interface : les listes
  sont séparées (v5) ; ne les fusionne pas « par commodité ».

## 5. Ce qui a toujours raison au conflit

Le README, puis docs/01-08, puis le code. Si un document contredit le
code EXÉCUTÉ, le document a tort et doit être fixé le jour même —
l'inverse est un bug.
