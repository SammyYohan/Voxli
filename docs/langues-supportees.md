# Langues prises en charge

> Ce que le produit SAIT aujourd'hui, par moteur. Seule la liste des codes
> de langue de Whisper est exhaustive ici — elle EST exactement la liste
> déroulante de Réglages. Pour Parakeet et Nemotron, la liste exacte des
> langues est la propriété du modèle : le dépôt ne l'invente pas — voir
> liens en bas.

---

## 1. Modification de la langue de dictée (Whisper)

Deux réglages DISTINCTS existent (schéma v5) :

- **Langue de l'interface** (`uiLanguage`) : `fr` ou `en` — règle les
  textes affichés, n'affecte en rien la reconnaissance.
- **Langue de dictée** (`language`) : `auto` (défaut, détection) ou un
  code de langue — règle l'INFÉRENCE linguistique de l'ASR.

`auto` est l'option recommandée ; garde-fou RF-16 : un énoncé plus court
que `autoLanguageMin` (défaut 1,5 s) retombe sur la langue de repli plutôt
que de deviner.

### Codes proposés dans l'interface (Whisper, 95 langues + auto)

auto (détection) · sq albanais · de allemand · am amharique · en anglais ·
ar arabe · hy arménien · as assamais · az azéri · ba bachkir · eu basque ·
be biélorusse · bn bengali · my birman · bs bosnien · br breton ·
bg bulgare · yue cantonais · ca catalan · zh chinois mandarin · ko coréen ·
ht créole haïtien · hr croate · si cingalais · da danois · es espagnol ·
et estonien · fo féroïen · fi finlandais · gl galicien · cy gallois ·
ka géorgien · el grec · gu gujarati · ha haoussa · haw hawaïen ·
he hébreu · hi hindi · hu hongrois · id indonésien · is islandais ·
it italien · ja japonais · jw javanais · kn kannada · kk kazakh ·
km khmer · lo lao · la latin · lv letton · ln lingala · lt lituanien ·
lb luxembourgeois · mk macédonien · ms malais · ml malayalam ·
mg malgache · mt maltais · mi maori · mr marathi · mn mongol ·
nn nynorsk norvégien · no norvégien · ne népalais · nl néerlandais ·
oc occitan · ur ourdou · uz ouzbek · ps pachto · pa pendjabi · fa persan ·
pl polonais · pt portugais · ro roumain · ru russe · sa sanskrit ·
sr serbe · sd sindhi · sk slovaque · sl slovène · sn shona · so somali ·
su soundanais · sv suédois · sw swahili · tg tadjik · tl tagalog ·
ta tamoul · tt tatar · cs tchèque · te telugu · th thaï · bo tibétain ·
tr turc · tk turkmène · uk ukrainien · vi vietnamien · yi yiddish ·
yo yoruba

(Le libellé vient de l'interface ; la valeur de configuration reste le
code seul — ex. « fr », « en ».)

## 2. Limites par moteur

- **Whisper (base/small quantisé)** : multilingue — la liste ci-dessus
  provient de l'interface ; la QUALITÉ varie selon la taille du modèle
  (le small est nettement meilleur sur les langues peu dotées) [À MESURER].
- **Parakeet TDT v3 0.6B (ONNX)** : 25 langues, ponctuation et casse
  natives. La liste exacte est celle du modèle publié
  (sherpa-onnx-nemo-parakeet-tdt-0.6b-v3). L'amorce de vocabulaire
  (RF-24) n'existe pas pour ce moteur — les néologismes passent par le
  dictionnaire post-traitement (choix documenté à l'écran).
- **Nemotron 3.5 streaming (SDK in-process)** : 40 langues revendiquées
  par le catalogue du modèle ; texte en direct (amendement ADR-02). La
  ponctuation automatique reste DÉSACTIVÉE tant que le modèle PNC
  complémentaire n'est pas câblé au téléchargement (choix documenté,
  jamais silencieux).

Aucune prétention de qualité égale entre moteurs : le choix reste dans
Réglages → Voix, avec la licence affichée avant téléchargement.

## 3. Langues de l'INTERFACE (i18n)

`fr` (défaut) et `en`. Tout autre valeur de `uiLanguage` retombe sur `fr`
(repli lisible, jamais d'écran abscons). Contribuer à une langue
supplémentaire = ajouter les clés dans `apps/desktop/src/i18n.ts` et une
option au sélecteur — voir CONTRIBUTING.md.
