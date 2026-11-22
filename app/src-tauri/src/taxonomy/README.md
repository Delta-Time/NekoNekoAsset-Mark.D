# Taxonomy and avatar evidence

Current contract version: `2026-11-03.15`. Public identity research was recorded
on 2026-10-09 and 2026-10-10 (Asia/Tokyo). This directory contains original Rust
classification/name rules, ordinary-language tag vocabulary, curated factual
identities and a separately frozen finite evaluation. Research provenance is below;
dependency notices are in `app/src-tauri/resources/licenses/THIRD-PARTY-NOTICE.txt`.

## Classification API

- `infer_category(&Value, &[String]) -> Value` returns
  `{category,basis,inferred,evidence}` without mutating its inputs. Approved
  keys are `Base`, `Clothes`, `Hair`, `Accessory`, `Texture`, `Motion`, `Tool`,
  `Shader`, `Gimmick`, `World`, `Udon` and `NonVRChat`. Source leaf/tag/parent
  strengths are 6/4/2; specific component evidence adds 5 and ties follow the
  fixed rule order. Texture covers Texture/Material. No match yields inferred
  `NonVRChat`, whose display label is Unclassified. Broad generic 3D evidence
  alone remains fallback. Classification establishes no ownership, file access,
  compatibility or readiness.
- `infer_product_category(&Value) -> Value` adds `scope`
  (`vrchat`, `unity`, `other`, `unknown`) and `scopeEvidence`. Retained
  `sourceTags` and `tags` contribute; description falls back to `description`
  when `sourceDescription` is empty. Generic 素材 does not imply Texture;
  explicit Shader/Motion evidence beats generic materials. Tool/Texture require
  VRChat or meaningful Unity context. Strong unrelated application evidence
  requires an explicit retained VRChat tag; Windows/macOS alone does not
  override genuine Unity context. Dependency mentions alone are insufficient.
  Evidence is bounded to eight entries of 240 characters.

An affirmative software type (software leaf, addon/plugin/editor-extension
name, or short software definition) takes precedence over words describing its
output. Explicit Shader/Udon types retain their specialized category. Blender
alone proves neither software type nor VRChat scope: an avatar may include
Blender sources, while a Blender-only addon can remain Unclassified. Base needs
an actual leaf category or affirmative named product type; broad avatar tags,
parent categories, compatibility/dependency descriptions and Avatar Configuration
are insufficient. Catalogue owns manual category overrides and raw observations.

BOOTH's own category decides before tags, which also list what a product contains
or suits. `3D衣装` and `3D靴` are Clothes, `3D髪型` Hair, `3D装飾品` and `3D小道具`
Accessory, `3Dモーション・アニメーション` Motion, `3Dテクスチャ` Texture,
`3Dツール・システム` and `ソフトウェア` Tool and `3D環境・ワールド` World, whatever a tag says.

BOOTH has no category for what a product is built on, so sellers file these together.
**Udon** goes into the world SDK, **Gimmick** into the avatar SDK, **World** is an
environment or a prop placed in one, and **Shader** includes the particles and
volumetric light placed in a world. A title word counts only where it names what is
sold rather than what the product comes with or suits (`ギミック付き`, `Udon対応`,
`シェーダー用`, `Udon不使用`, `同梱`). Under worlds, tools and software a title naming `udon` or
a Udon system (`CVS2`, `チクワカー`) is Udon, unless it sells a tool or calls itself a world
over tags showing a place, and so is a gimmick or system a Udon tag explains. A title that
calls itself a world then stops at World, so a finished world keeps its Udon tags to itself.
A title selling a shader or volumetric light is Shader, as are particles under worlds unless
the title says avatar gimmick. A world gimmick tag is Udon unless the tags show a room, and
it counts in every category but `3Dキャラクター`. Under tools and software `ワールドアセット` or
`ワールドキット` is Udon too, as is any Udon tag unless the title sells a tool or a tag says
editor. A title selling a tool is Tool. Under worlds a sold gimmick is Udon unless it is for
an avatar, an avatar tag is Gimmick, and a shader tag is Shader only with a light or effect
word and no tags showing a place. Elsewhere a sold gimmick is Gimmick unless tags say both
editor and tool, and a title or tag with `アバターギミック`, `ModularAvatar`, `MA対応` or
`Avatar3.0` is too, unless a tag says editor or tool without also saying gimmick. Only tools read plain
Udon, shader and gimmick tags when nothing else decided. In `3D小道具` and `3D装飾品` a
title selling a Udon gimmick is Udon and one saying `ワールド小物` is World. In
`3Dモデル（その他）` a world tag beats a 小物 tag if nothing mentions avatars or
accessories. A title in `3Dキャラクター` that ends by naming a texture is Texture, and
under 小説・その他書籍, 漫画, 音楽, 音声作品 and イラスト tags name no type at all.

A `3Dテクスチャ` title that
sells a shader and does not also say texture is Shader. `販売ワールド` in a title is
World in any category without a type of its own. A 3D category other than tools needs
no VRChat or Unity word to stay what it says, and a Photoshop or Windows tag does not
take it out of the default list, though a title naming that application does. Anything
in tools still has to show where it runs. Add-on wording or a software tag does not
make clothing, hair, accessories or textures software; `Blender`, `エディタ拡張` or a
description defining one does. `3Dキャラクター` keeps its own rule below and shares this
default-list treatment.

Under any other top-level category (`素材データ`, `ゲーム`, `映像作品`, `グッズ` ...) a
word needs VRChat or Unity evidence as well. `映像作品 / アニメーション` is BOOTH's video
shelf, and bare アニメーション also describes GIFs, moving backgrounds and particle
effects, so outside 3D categories it counts as motion only with that evidence and no
video or effect word beside it. Motion wording (モーション, emote, pose, an animation
clip) counts by itself, except モーショングラフィックス. Tags and titles reach World only
through ワールド or world, not 世界 as in 世界観 or 異世界, and outside 3D categories not on
a product with effect words. A BOOTH description names a type only by opening with a
definition of a shader or software, because it mentions dependencies, contents and what
a product goes with. All of this is BOOTH's: other stores keep their own rules. A manual
category is an overlay applied after all of this and never moves.

BOOTH's `3Dキャラクター` leaf makes a product Base whatever its tags say, unless
software is declared, by the maintainer's choice of the category alone. `infer_category`
keeps its scores because storage freezes its answer in managed paths. With no leaf
or only `3Dモデル（その他）`, tags and descriptions do not outrank a title that
calls itself one named model: `オリジナル3Dモデル`, `Original 3D Model` or VRChat/VRC
with 向け/用/対応 and 3Dモデル/モデル/アバター must introduce the name or directly
follow a quoted one. Add-on, compatibility or other product wording, a missing name
and a bare `3Dモデル` stay out; product wording after + or & can be a bonus, so
there only the name parser decides. A removed page proves
nothing. A rule change reaches a stored product when its own records change, not at
opening, so it needs no `VERSION` bump.

## Identity, names and tags

- `dictionary() -> Value` exposes the current version, 32 public identities,
  category keys, original style/use/rating vocabulary and the frozen evaluation.
  `populationCoverage` remains null.
- `normalize` and `aliases(&str) -> Vec<String>` provide deterministic
  NFKC/case/whitespace normalization and deduplicated index alternatives.
  Aliases are alternatives, not an AND query or filename-to-product lookup.
- `match_avatars(&str) -> Vec<Value>` returns lexical candidates with stable
  `booth:<numeric-ID>`, original names, matched spellings and source provenance.
  ASCII fragments inside longer words and CJK fragments such as 萌 inside 萌芽
  are excluded. Delimited filename mentions and explicit compatibility suffixes
  can produce candidates; they do not establish product identity or acquisition.
- `exact_avatar_tag(&str) -> Option<Value>` accepts complete recognized aliases,
  bilingual pairs, wrappers and compatibility suffixes, returning
  `basis:exactTag`, `matchedAliases` and `entitlementEstablished:false`.
  Apparel substrings, numeric IDs and mixed two-avatar labels do not qualify.
- `owned_base_options(&[Value]) -> Vec<Value>` supplements choices from supplied
  effective Base products. Deduplication uses explicit product ID, never title.
  Known IDs retain dictionary names; unknown IDs retain their source identity
  and reading provenance even when a label equals a curated name. This helper
  does not query SQLite, scan files or fill `ownedAvatars`.
- `clean_avatar_name`, `avatar_label_parts`, `avatar_label_is_unambiguous`,
  `romanized_name` and `avatar_display_name` share conservative presentation
  parsing. Complete marketing/platform wrappers and complete bilingual pairs
  can be consumed; unknown qualifiers remain. English uses curated spellings,
  source-title spelling, deterministic kana romanization or cached local Windows
  readings with distinct provenance. Unsupported readings remain empty and need
  a user override. Display is Original / English when available, otherwise
  Original; no Original / — placeholder or spelling-based identity merge.
- `parsed_product_avatar_label` reads the title of a product already known to be
  a Base, so it locates the name instead of recognizing every word around it. A
  complete 「」『』“”"" quote outranks all surrounding text; otherwise free text
  outranks 【】[] groups, which supply the name only when nothing else can.
  Sale, status, license and platform text needs no entry to be left out; a Latin
  spelling is accepted on either side across any separator or bracket. Status text
  after a dash that follows the model wording lists what the model includes. Add-on
  wording, text directly after a quote saying who it is for (用, 向け, 対応, 専用), two
  names or no name keep the visible label as `fallback`, and several named avatars
  become `bundle`, so neither creates an automatic alias. A listing marked 版 or 無料
  beside its name leaves a plain listing's name unambiguous, like an edition. Arbitrary
  evidence labels and identity hashes keep the conservative parsers. Changing
  these rules needs a `VERSION` bump: a library re-resolves its stored tags once,
  at opening, per version, without reading any store or source record.
- `canonical_tag`, `effective_tags` and `tag_suggestions` implement alias
  equivalence, normalized deduplication and rejected-tag filtering. Suggestions
  do not change sensitivity or entitlements. Catalogue owns durable overlays,
  rejection and compatibility override precedence.

`AvatarTagResolver::from_options(&[Value])` builds an immutable exact lookup from
canonical dictionary and effective owned Base options after the caller applies
identity links and user names/aliases. Complete normalized names map to sets of
canonical IDs. `resolve_tag` returns a compatibility candidate only for one ID;
shared-name collisions, including curated/owned collisions, return None.
A plain product keeps its name when it collides only with its own qualified
editions. Repeated same-ID entries contribute aliases. A complete same-identity bilingual
pair can distinguish shared original names; partial pairs, garments, substrings,
internal IDs, numbers and URLs cannot establish compatibility. Snapshot creation
indexes names/pairs once; tag resolution does not scan all Bases.

The resolver accepts one `ちゃん` after a complete name/alias, optionally with an
existing compatibility suffix. Full and honorific-stripped spellings both
contribute candidates, so a real NAMEちゃん identity colliding with NAME remains
ambiguous. Repetition, arbitrary sentences and additional suffix peeling are
excluded. Its `canonical_tag` uses the same snapshot for rejection equivalence;
ambiguous/unknown text stays plainly normalized, without a global fallback that
could conceal a collision. Catalogue owns snapshot lifetime/invalidation and
preserves explicit overrides. All returned candidates retain
`entitlementEstablished:false`.

## Public support and account compatibility

`variant_avatars(&str)` interprets positive base-specific offer names, including
explicit NAME専用 before that name is curated. Generic platform/colour/audience
variants, negative or future support statements and unrelated credits/dependencies
do not establish support. `is_all_inclusive_variant(&str)` recognizes explicit
whole-product/all-avatar bundles; constrained packs do not expand to all bases.

`listing_avatar_evidence(&Value)` combines public digital offers, retained
positive tags/title and explicit supported-avatar sections/linked base identities.
The source pipeline retains public variation IDs, names and types, excluding
order URLs, purchase counts, shipping attributes and downloadable controls.
BOOTH JSON can omit sectioned description text; absence there does not imply
unsupported. Conditional public HTML enrichment reads the supported sections.
Fresh explicit support replaces prior support, including removals and empty
sets. A successful page with the established `section.shop__text` description
scope can confirm section removal; unrelated/login/error bodies preserve
last-known evidence. JSON-only request decisions do not suppress enrichment
because an earlier HTML result exists.

`account_variant_avatars(&Value, Option<&str>, &str)` operates on a currently
accessible account file's observed variant. Exact public digital variant ID,
or a unique normalized exact offer name when ID is absent, can bind a full
bundle to all proven public supported bases. Single-base and constrained packs
keep scoped evidence; a variant label can support its own named base without
expanding an unbound FullPack. Ambiguous offer matches return no evidence.
Manual compatibility overrides win. Empty account selection supplies no account
evidence; withdrawing accessible files removes their account evidence. Public
support creates no acquisitions, remote files, ownership or entitlement. New
profiles derive compatibility directly from observations; no migration runs.

## Factual identity provenance and licensing

[avatars.json](avatars.json) contains 32 independently curated product identities,
with official public BOOTH source URLs, original names and per-English-name
`englishNameEvidence`. There are 28 creator-attested English spellings, three
maintainer-approved romanization fallbacks and one user-accepted spelling. Observation
dates and runtime provenance distinguish this dated snapshot from a claim of
current purchase availability.

| Original / English | Creator | Official identity source | English status |
| --- | --- | --- | --- |
| 愛莉 / Airi | キュビクローゼット | [6082686](https://kyubihome.booth.pm/items/6082686) | official |
| 舞夜 / Maya | キュビクローゼット | [3390957](https://kyubihome.booth.pm/items/3390957) | official |
| 萌 / Moe | キュビクローゼット | [4667400](https://kyubihome.booth.pm/items/4667400) | romanizationFallback |
| しなの / Shinano | ポンデロニウム研究所 | [6106863](https://ponderogen.booth.pm/items/6106863) | romanizationFallback |
| 桔梗 / Kikyo | ポンデロニウム研究所 | [3681787](https://ponderogen.booth.pm/items/3681787) | romanizationFallback |
| マヌカ / Manuka | STUDIO JINGO | [5058077](https://jingo1016.booth.pm/items/5058077) | official |
| セレスティア / SELESTIA | STUDIO JINGO | [4035411](https://jingo1016.booth.pm/items/4035411) | official |
| ひきくまりのクマリ / Kumaly | STUDIO JINGO | [7928592](https://jingo1016.booth.pm/items/7928592) | official |
| シフォン / Chiffon | あまとうさぎ | [5354471](https://komado.booth.pm/items/5354471) | official |
| ライム / Lime | あまとうさぎ | [4876459](https://komado.booth.pm/items/4876459) | official |
| ショコラ / Chocolat | あまとうさぎ | [6405390](https://komado.booth.pm/items/6405390) | official |
| プラム / Plum | あまとうさぎ | [7770415](https://komado.booth.pm/items/7770415) | official |
| カリン / Karin | あまとうさぎ | [3470989](https://komado.booth.pm/items/3470989) | official |
| ラスク / Rusk | あまとうさぎ | [2559783](https://komado.booth.pm/items/2559783) | official |
| ミルク Re / Milk Re | あまとうさぎ | [2953391](https://komado.booth.pm/items/2953391) | official |
| ミント / Mint | あまとうさぎ | [2258111](https://komado.booth.pm/items/2258111) | official |
| 真冬 / Mafuyu | ぷらすわん | [5007531](https://booth.pm/ja/items/5007531) | official |
| まめひなた / Mamehinata | もち山金魚 | [4340548](https://mukumi.booth.pm/items/4340548) | official |
| ミルティナ / Milltina | DOLOS art | [6538026](https://dolosart.booth.pm/items/6538026) | official |
| 瑞希 / mizuki | IKUSIA | [5132797](https://paryi.booth.pm/items/5132797) | official |
| ルルネ / rurune | IKUSIA | [5957830](https://booth.pm/ja/items/5957830) | official |
| ミルフィ / Milfy | ぷらすわん | [6571299](https://mk22.booth.pm/items/6571299) | official |
| ミーシェ / Mishe | ポンデロニウム研究所 | [1256087](https://ponderogen.booth.pm/items/1256087) | userAccepted |
| 竜胆 / Rindo | STUDIO JINGO | [3443188](https://jingo1016.booth.pm/items/3443188) | official |
| リーファ / Leefa | ひゅうがなつみかん | [3659436](https://hyuuganatu.booth.pm/items/3659436) | official |
| あのん / Anon | こよりLabo | [3564947](https://nagatorokoyori.booth.pm/items/3564947) | official |
| りりか / ririka | IKUSIA | [6373683](https://paryi.booth.pm/items/6373683) | official |
| 真央 / mao | IKUSIA | [6846646](https://paryi.booth.pm/items/6846646) | official |
| しお / Sio | Chocolate rice | [5650156](https://chocolaterice.booth.pm/items/5650156) | official |
| ここあ / Kokoa | キュビクローゼット | [2780069](https://kyubihome.booth.pm/items/2780069) | official |
| Lapwing / Lapwing | 久 | [4993931](https://kujishift.booth.pm/items/4993931) | official |
| マリシア / Marycia | BeroarN | [6305948](https://brn.booth.pm/items/6305948) | official |

Creator-attested spellings come from titles, model headings, public package
identifiers, installation/update notes and tags. Case is preserved. Specific
non-title evidence includes Airi's gesture note, Maya's tag, Milltina's model
heading, mizuki's body heading, Jingo's Manuka/SELESTIA/Kumaly/Rindo update
identifiers, Leefa's prefab rename, Anon's install paths and Kokoa's model
specification. Lapwing and Marycia were checked on 2026-10-10. Official image
lettering was not asserted because that research image viewer was unavailable.

Moe, Shinano and Kikyo remain fallbacks without creator Latin attestation.
Mishe was supplied by the maintainer and is corroborated by
[the clothing author's listing](https://booth.pm/ja/items/2873084), which cites
original model 1256087; its avatar-author page establishes the Japanese identity,
not official English. Miishie, Riifa and Shio retain prior search spellings of
the same identities; Mafuyu/Mahuyu equivalence is also search evidence. Local
Windows phonetic readings are presentation fallbacks, not authoritative proper
names. Public Hakka routes were not verifiable at research time and no identity
was added.

No descriptions, images, meshes, archives, asset license text or third-party
rule/tag tables are bundled. Public identification grants no rights to underlying
assets or branding; creators retain their applicable rights. MIT covers the
project's original code, vocabulary and curation, not third-party assets.
No MioVRCA, VRCNext or AGPL reference code/data was copied. The unauthenticated
BOOTH `/ja/items/<ID>.json` endpoint exposes title/tags but can omit sectioned
body text; full official pages supplied the relevant name/support observations.
No authenticated source or private corpus was used for this dictionary research.

## Public regression provenance

Existing fixtures are independently authored minimal structural patterns based
on public source facts, not copied product records. Their finite examples do not
measure population classification accuracy or grant asset rights.

| Official listing | Relevant public evidence | Expected category |
| --- | --- | --- |
| [7329831](https://booth.pm/ja/items/7329831) | AfterEffects/AE/Windows tool | Unclassified |
| [7969637](https://booth.pm/ja/items/7969637) | NovelAI/Windows software | Unclassified |
| [3282463](https://booth.pm/ja/items/3282463) | Streaming desk PNG illustration materials | Unclassified |
| [2153720](https://booth.pm/ja/items/2153720) | Voice samples; ボイス leaf | Unclassified |
| [1906822](https://booth.pm/ja/items/1906822) | Novel/doujin cover image materials | Unclassified |
| [2662823](https://booth.pm/ja/items/2662823) | TRPG/CCFOLIA APNG materials | Unclassified |
| [4070235](https://booth.pm/ja/items/4070235) | AE text preset; font category | Unclassified |
| [2826022](https://booth.pm/ja/items/2826022) | Explicit Unity/VRChat/shader tags | Shader |
| [2832898](https://booth.pm/ja/items/2832898) | Explicit Unity/VRChat/shader tags | Shader |
| [3776089](https://booth.pm/ja/items/3776089) | Unity/VRChat Humanoid motion tags | Motion |
| [3799864](https://booth.pm/ja/items/3799864) | Unity/VRChat Humanoid motion tags | Motion |
| [8643321](https://tohawork.booth.pm/items/8643321) | Declared Blender shape addon with VRChat support | Tool / vrchat |
| [8606444](https://booth.pm/ja/items/8606444) | Declared Unity MaskMaker software | Tool / unity |

The first seven examples lack retained VRChat tags; generic 素材 does not prove
Unity Texture. Generic item routes were readable when legacy nanakorobi-hi shop
routes were unavailable. The public
[8198362 listing](https://booth.pm/ja/items/8198362) supplied five digital offers
and a four-base support section during research. It motivated general public/
account support handling; there is no product-ID dispatch.

## Frozen finite evaluation and verification scope

[evaluation.json](evaluation.json) records the original `2026-10-09.1` snapshot,
not a fresh evaluation of all 32 current identities. Before viewing that
snapshot, ten names were nominated independently: マヌカ, 萌, 桔梗,
セレスティア, まめひなた, ライム, 森羅, しなの, 瑞希 and ソラハ. It was an
unweighted convenience sample, without a user corpus or popularity ranking.
The frozen denominator is ten, with eight recorded identity/name matches and
nine primary identity pages verified at research time.

[ソラハ](https://kokokoya.booth.pm/items/4208278) is a verified unmatched holdout.
[森羅's route](https://mio3works.booth.pm/items/4707634), including the generic
item route, returned 404 during research; its candidate ID remains explicitly
unverified against the primary listing and stays an unmatched denominator row.
Neither identity is bundled. Eight of these ten is not 80% of ordinary users'
needs, full-corpus accuracy, or compatibility/entitlement verification. Later
additions leave this original record unchanged; future independent evaluations
need separately selected cases rather than tuning and reusing this holdout.

Existing native tests cover classification/type/scope, normalization/boundaries,
name provenance, ambiguity and rejection, Base supplementation, public/account
separation, bundle binding and the frozen evaluation. Sequential support fixtures
cover 4→3→0 replacement, unrelated-body retention and removed-section withdrawal;
existing catalogue fixtures cover account filtering, overlays, accessible-file
withdrawal and restart. These notes report no test run, live-account validation or
rendered-quality acceptance.
