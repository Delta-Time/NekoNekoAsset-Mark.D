//! Original fixed taxonomy and curated public avatar identities.
//! Matches are evidence-bearing candidates, never acquisition or entitlement facts.
mod compatibility;
pub use compatibility::{
    account_variant_avatars, is_all_inclusive_variant, listing_avatar_evidence, variant_avatars,
};
mod names;
mod phonetic;
mod resolver;
pub use resolver::AvatarTagResolver;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

pub const VERSION: &str = "2026-11-03.15";
pub const CATEGORY_KEYS: &[&str] = &[
    "Base",
    "Clothes",
    "Hair",
    "Accessory",
    "Texture",
    "Motion",
    "Tool",
    "Shader",
    "Gimmick",
    "World",
    "Udon",
    "NonVRChat",
];

/// Search normalization retains original-script identity; no transliteration or
/// simplified/traditional kanji guessing is performed.
pub fn normalize(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

struct Rule {
    category: &'static str,
    specific: bool,
    words: &'static [&'static str],
    fragments: &'static [&'static str],
}

// Priority and weights preserve the accepted prototype: specific component
// evidence beats a broad avatar/world parent; equal scores use this stable order.
const RULES: &[Rule] = &[
    Rule {
        category: "Udon",
        specific: true,
        words: &["udon", "udonsharp"],
        fragments: &["udonシステム", "udon系统", "udon系統"],
    },
    Rule {
        category: "Shader",
        specific: true,
        words: &["shader", "shaders"],
        fragments: &["シェーダ", "셰이더", "着色器", "著色器"],
    },
    Rule {
        category: "Hair",
        specific: true,
        words: &["hair", "hairstyle", "hairstyles"],
        fragments: &["髪型", "ヘア", "헤어", "发型", "髮型"],
    },
    Rule {
        category: "Clothes",
        specific: true,
        words: &[
            "cloth", "clothes", "clothing", "outfit", "outfits", "costume", "costumes", "coat",
            "coats", "dress", "dresses",
        ],
        fragments: &["衣装", "衣服", "服装", "服裝", "의상"],
    },
    Rule {
        category: "Accessory",
        specific: true,
        words: &[
            "accessory",
            "accessories",
            "prop",
            "props",
            "brooch",
            "brooches",
            "jewelry",
            "jewellery",
        ],
        fragments: &[
            "アクセサリー",
            "小物",
            "小道具",
            "装飾品",
            "配饰",
            "配飾",
            "액세서리",
        ],
    },
    Rule {
        category: "Texture",
        specific: true,
        words: &["texture", "textures", "material", "materials"],
        fragments: &[
            "テクスチャ",
            "マテリアル",
            "贴图",
            "貼圖",
            "材质",
            "材質",
            "텍스처",
            "머티리얼",
        ],
    },
    Rule {
        category: "Motion",
        specific: true,
        words: &[
            "motion",
            "motions",
            "animation",
            "animations",
            "gesture",
            "gestures",
            "pose",
            "poses",
        ],
        fragments: &[
            "モーション",
            "アニメーション",
            "动作",
            "動作",
            "모션",
            "애니메이션",
        ],
    },
    Rule {
        category: "Tool",
        specific: true,
        words: &["tool", "tools", "utility", "utilities", "editor extension"],
        fragments: &["ツール", "工具", "도구"],
    },
    Rule {
        category: "Gimmick",
        specific: true,
        words: &["gimmick", "gimmicks", "interaction", "interactions"],
        fragments: &["ギミック", "交互组件", "互動元件", "기믹"],
    },
    Rule {
        category: "World",
        specific: false,
        words: &["world", "worlds", "environment", "environments"],
        fragments: &["ワールド", "世界", "월드"],
    },
    Rule {
        category: "Base",
        specific: false,
        words: &[
            "avatar",
            "avatars",
            "base avatar",
            "3d character",
            "3d characters",
        ],
        fragments: &["アバター", "3dキャラクター", "素体", "素體", "아바타"],
    },
    Rule {
        category: "NonVRChat",
        specific: true,
        words: &[
            "nonvrchat",
            "non-vrchat",
            "non vrchat",
            "brush",
            "brushes",
            "print",
            "printing",
            "illustration",
            "illustrations",
        ],
        fragments: &["vrchat以外", "イラスト", "笔刷", "筆刷"],
    },
];

fn word_match(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(start, _)| {
        let end = start + word.len();
        !text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric())
            && !text[end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

fn rule_match(text: &str, rule: &Rule) -> bool {
    rule.words.iter().any(|word| word_match(text, word))
        || rule.fragments.iter().any(|word| text.contains(word))
}

fn evidence_excerpt(text: &str) -> String {
    text.chars().take(240).collect()
}

/// Accepts either a source-category string or {name,parentName,...}. Unknown
/// fields remain in the caller's source record; raw input is never modified.
pub fn infer_category(source_category: &Value, tags: &[String]) -> Value {
    let leaf = source_category
        .as_str()
        .or_else(|| source_category["name"].as_str())
        .unwrap_or("");
    let parent = source_category["parentName"].as_str().unwrap_or("");
    let normalized_leaf = normalize(leaf);
    let normalized_parent = normalize(parent);
    let normalized_tags: Vec<_> = tags.iter().map(|tag| normalize(tag)).collect();
    let mut winner =
        json!({"category":"NonVRChat", "basis":"fallback", "inferred":true, "evidence":[]});
    let mut best = 0;
    for rule in RULES {
        let leaf_match = rule_match(&normalized_leaf, rule);
        let parent_match = rule_match(&normalized_parent, rule);
        let matching_tags: Vec<_> = normalized_tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| rule_match(tag, rule))
            .map(|(i, _)| i)
            .collect();
        let tag_match = !matching_tags.is_empty();
        let strength = if leaf_match {
            6
        } else if tag_match {
            4
        } else if parent_match {
            2
        } else {
            0
        };
        let score = strength + if strength > 0 && rule.specific { 5 } else { 0 };
        if score > best {
            let mut evidence = Vec::new();
            if leaf_match {
                evidence.push(json!({"field":"sourceCategory.name", "value":leaf}));
            }
            if parent_match {
                evidence.push(json!({"field":"sourceCategory.parentName", "value":parent}));
            }
            evidence.extend(
                matching_tags
                    .iter()
                    .map(|i| json!({"field":"sourceTags", "value":tags[*i]})),
            );
            winner = json!({"category":rule.category,
                "basis":if leaf_match || parent_match && !tag_match { "sourceCategory" } else { "tags" },
                "inferred":true, "evidence":evidence});
            best = score;
        }
    }
    winner
}

/// Production classification uses retained public metadata, never availability,
/// filenames or private payloads. Manual category overlays are applied later.
pub fn infer_product_category(source: &Value) -> Value {
    let text = |key: &str| source[key].as_str().unwrap_or("");
    let title = ["sourceTitle", "title", "name"]
        .iter()
        .map(|k| text(k))
        .find(|v| !v.trim().is_empty())
        .unwrap_or("");
    let description = ["sourceDescription", "description"]
        .iter()
        .map(|k| text(k))
        .find(|v| !v.trim().is_empty())
        .unwrap_or("");
    let tags: Vec<String> = source_tags(source);
    let normalized_tags: Vec<_> = tags.iter().map(|v| normalize(v)).collect();
    let title_normalized = normalize(title);
    let description_normalized = normalize(description);
    let jinxxy = ["provider", "store"]
        .iter()
        .any(|key| normalize(text(key)) == "jinxxy");
    let gumroad = ["provider", "store"]
        .iter()
        .any(|key| normalize(text(key)) == "gumroad");
    let has_vr = |v: &str| {
        word_match(v, "vrchat")
            || word_match(v, "vrc")
            || word_match(v, "vrcfury")
            || v.contains("vrチャット")
    };
    let explicit_vr_tag = normalized_tags.iter().any(|v| has_vr(v));
    let explicit_vr_text = has_vr(&title_normalized) || has_vr(&description_normalized);
    let vr_scope = jinxxy || (!gumroad && explicit_vr_tag) || explicit_vr_text;
    let unity_scope = in_unity_scope(&normalized_tags, &title_normalized, &description_normalized);
    let outside_app_tag = tagged_outside_app(&normalized_tags);
    // Product-opening context is useful even when sellers omit application
    // tags. Do not scan long credits/dependency sections or treat OS names as
    // strong application evidence.
    let description_opening: String = description_normalized.chars().take(240).collect();
    let outside_app_context = outside_app_tag
        || strong_external_application(&title_normalized)
        || strong_external_application(&description_opening);
    let os_tag = normalized_tags
        .iter()
        .any(|tag| matches!(tag.as_str(), "windows" | "macos"));
    let source_category = &source["sourceCategory"];
    let jinxxy_addon = jinxxy
        && source_category["types"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|kind| {
                kind.as_str()
                    .is_some_and(|kind| normalize(kind) == "avatar_addon")
            });
    let category_name = normalize(
        source_category
            .as_str()
            .or_else(|| source_category["name"].as_str())
            .unwrap_or(""),
    );
    let outside_category = category_outside_scope(&category_name);
    let target_scope = vr_scope || unity_scope;
    // BOOTH's top level says whether a product is 3D data at all.
    let booth = ["provider", "store"]
        .iter()
        .any(|key| normalize(text(key)) == "booth");
    let parent_name = normalize(source_category["parentName"].as_str().unwrap_or(""));
    let in_3d = parent_name == "3dモデル";
    // Bare アニメーション also describes videos, GIFs and particle effects, and a store
    // category by that name is BOOTH's video shelf. Outside 3D categories it counts only
    // in VRChat or Unity context with none of those words beside it.
    let video_or_effect = std::iter::once(&title_normalized)
        .chain(&normalized_tags)
        .any(|text| mentions_video_or_effect(text));
    let evidence = |text: &str, rule: &Rule| match rule.category {
        "Motion" if booth => {
            names_motion(text)
                || names_emote_or_pose(text) && (in_3d || target_scope)
                || bare_animation(text) && (in_3d || target_scope && !video_or_effect)
        }
        // 世界 is as often 世界観 or 異世界, and effect sellers tag every platform.
        "World" if booth => {
            (in_3d || !video_or_effect) && rule_match(&text.replace("世界", ""), rule)
        }
        _ => rule_match(text, rule),
    };
    let counts = |text: &str| {
        !RULES.iter().any(|rule| rule_match(text, rule))
            || RULES.iter().any(|rule| evidence(text, rule))
    };
    let type_tags: Vec<String> = tags
        .iter()
        .zip(&normalized_tags)
        .filter(|(_, tag)| counts(tag))
        .map(|(tag, _)| tag.clone())
        .collect();
    let mut result = infer_category(
        if counts(&category_name) {
            source_category
        } else {
            &Value::Null
        },
        &type_tags,
    );
    let store = booth
        .then(|| {
            store_type(
                &category_name,
                &title_normalized,
                &normalized_tags,
                &description_opening,
            )
        })
        .flatten();
    let facts = Facts {
        title,
        description,
        tags: &tags,
        normalized_tags: &normalized_tags,
        title_normalized: &title_normalized,
        description_normalized: &description_normalized,
        description_opening: &description_opening,
        source_category,
        category_name: &category_name,
        parent_name: &parent_name,
        booth,
        jinxxy,
        gumroad,
        jinxxy_addon,
        in_3d,
        vr_scope,
        unity_scope,
        target_scope,
        explicit_vr_tag,
        explicit_vr_text,
        outside_app_context,
        outside_category,
        os_tag,
    };
    apply_store_type(&mut result, store, &facts);
    let declared_software = apply_software(&mut result, store, &facts);
    let base_title = apply_base(&mut result, store, declared_software, &facts);
    apply_tag_type(&mut result, store, &facts, &evidence);
    apply_fallback(&mut result, base_title, &facts, &evidence);
    apply_world_prop(&mut result, store, &facts);
    apply_scope(&mut result, store, &facts);
    result
}

/// The listing's tags and stored tags, deduplicated and sorted.
fn source_tags(source: &Value) -> Vec<String> {
    ["sourceTags", "tags"]
        .iter()
        .flat_map(|key| {
            source[*key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Whether a tag, the title or the description places the product in Unity.
fn in_unity_scope(
    normalized_tags: &[String],
    title_normalized: &str,
    description_normalized: &str,
) -> bool {
    let has_unity = |v: &str| {
        word_match(v, "unity")
            || word_match(v, "unity3d")
            || word_match(v, "udon")
            || word_match(v, "udonsharp")
            || v.contains("unitypackage")
    };
    // A passing mention of Unity in a long description is not sufficient. A
    // retained tag/title or actual engine/package vocabulary provides context.
    normalized_tags.iter().any(|v| has_unity(v))
        || has_unity(title_normalized)
        || has_unity(description_normalized)
            && [
                "unitypackage",
                "ugui",
                "humanoid",
                "shader",
                "シェーダ",
                "インポート",
                "import",
                ".anim",
                "prefab",
                "プレハブ",
                "editor extension",
                "エディタ拡張",
            ]
            .iter()
            .any(|term| description_normalized.contains(term))
}

/// Whether a tag names an application or medium outside VRChat and Unity.
fn tagged_outside_app(normalized_tags: &[String]) -> bool {
    normalized_tags.iter().any(|tag| {
        [
            "aftereffects",
            "after effects",
            "aftereffcts",
            "ae",
            "novelai",
            "photoshop",
            "clip studio",
            "clipstudiopaint",
            "live2d",
            "ココフォリア",
            "trpg",
            "coc",
            "voice",
            "ボイス",
        ]
        .iter()
        .any(|term| word_match(tag, term))
    })
}

/// Whether the store category is an illustration, voice or font shelf.
fn category_outside_scope(category_name: &str) -> bool {
    [
        "イラスト素材",
        "ボイス",
        "フォント",
        "書体",
        "illustration",
        "voice",
        "font",
    ]
    .iter()
    .any(|term| category_name.contains(term))
}

/// What the category stages read about one product, computed once by
/// `infer_product_category` from its retained public metadata.
#[derive(Clone, Copy)]
struct Facts<'a> {
    title: &'a str,
    description: &'a str,
    tags: &'a [String],
    normalized_tags: &'a [String],
    title_normalized: &'a str,
    description_normalized: &'a str,
    description_opening: &'a str,
    source_category: &'a Value,
    category_name: &'a str,
    parent_name: &'a str,
    booth: bool,
    jinxxy: bool,
    gumroad: bool,
    jinxxy_addon: bool,
    in_3d: bool,
    vr_scope: bool,
    unity_scope: bool,
    target_scope: bool,
    explicit_vr_tag: bool,
    explicit_vr_text: bool,
    outside_app_context: bool,
    outside_category: bool,
    os_tag: bool,
}

/// BOOTH's store category, read with the title and tags, names the type.
fn apply_store_type(
    result: &mut Value,
    store: Option<(&'static str, &'static str)>,
    facts: &Facts,
) {
    let Facts {
        title,
        tags,
        normalized_tags,
        source_category,
        ..
    } = *facts;
    if let Some((kind, basis)) = store {
        let found: Vec<Value> = match basis {
            "sourceTitle" => vec![json!({"field":"sourceTitle","value":title})],
            "sourceTags" => tags
                .iter()
                .zip(normalized_tags)
                .filter(|(_, tag)| {
                    names_type(tag, kind)
                        || kind == "Udon"
                            && (tag.contains("udon")
                                || word_match(tag, "u#")
                                || WORLD_GIMMICK_TAGS
                                    .iter()
                                    .chain(WORLD_BUILD_TAGS)
                                    .any(|word| tag.contains(word))
                                || tag.contains("チクワカーシステム")
                                || tag.contains("chikuwa vehicle system"))
                        || kind == "Gimmick" && names_avatar_build(tag)
                })
                .map(|(tag, _)| json!({"field":"sourceTags","value":tag}))
                .collect(),
            _ => vec![
                json!({"field":"sourceCategory.name","value":source_category.as_str().or_else(|| source_category["name"].as_str()).unwrap_or("")}),
            ],
        };
        *result = json!({"category":kind,"basis":basis,"inferred":true,"evidence":found});
    }
}

/// Declared software is a tool; returns whether the product declares itself software.
fn apply_software(
    result: &mut Value,
    store: Option<(&'static str, &'static str)>,
    facts: &Facts,
) -> bool {
    let Facts {
        title,
        description,
        tags,
        normalized_tags,
        title_normalized,
        description_opening,
        source_category,
        category_name,
        ..
    } = *facts;
    // Software describes the product; texture/avatar words often describe its
    // input or output. Only declared software type may override those words.
    let software_category = software_type(category_name);
    let software_title = software_type(title_normalized);
    let software_tag = normalized_tags.iter().any(|tag| software_type(tag));
    let software_opening = description_defines_software(description_opening);
    let software = software_category || software_title || software_tag || software_opening;
    let declared_software = software_category || software_title || software_opening;
    let specialized_software = RULES.iter().any(|rule| {
        matches!(rule.category, "Shader" | "Udon")
            && result["category"] == rule.category
            && rule_match(title_normalized, rule)
            && !software_title
            && !software_opening
            && !RULES
                .iter()
                .find(|r| r.category == "Tool")
                .is_some_and(|r| rule_match(title_normalized, r))
    });
    // In a garment, hair, accessory or texture category an add-on is one more pack,
    // and a software tag one more tag.
    let store_content = matches!(
        store,
        Some(("Clothes" | "Hair" | "Accessory" | "Texture", _)) | Some(("World", "sourceTitle"))
    );
    // What a title or tag already separated from tools says more than the word tool.
    let store_built_for = matches!(store, Some(("Udon" | "Gimmick" | "Shader", _)));
    let software_decides = if store_built_for {
        false
    } else if store_content {
        description_defines_software(&description_opening.replace("アドオンです", ""))
            || software_title && !add_on_pack(title_normalized)
    } else {
        software
            && (declared_software
                || matches!(
                    result["category"].as_str(),
                    Some("Base" | "Texture" | "NonVRChat" | "Tool")
                ))
    };
    if software_decides && !specialized_software {
        let (field, value) = if software_category {
            (
                "sourceCategory.name",
                source_category
                    .as_str()
                    .or_else(|| source_category["name"].as_str())
                    .unwrap_or(""),
            )
        } else if software_title {
            ("sourceTitle", title)
        } else if software_tag {
            (
                "sourceTags",
                tags.iter()
                    .find(|tag| software_type(&normalize(tag)))
                    .map(String::as_str)
                    .unwrap_or(""),
            )
        } else {
            ("sourceDescription", description)
        };
        *result = json!({"category":"Tool","basis":field,"inferred":true,"evidence":[{"field":field,"value":evidence_excerpt(value)}]});
    }
    declared_software
}

/// Only a store category or a title supplies an avatar; returns whether the title does.
fn apply_base(
    result: &mut Value,
    store: Option<(&'static str, &'static str)>,
    declared_software: bool,
    facts: &Facts,
) -> bool {
    let Facts {
        title,
        normalized_tags,
        title_normalized,
        source_category,
        category_name,
        jinxxy_addon,
        ..
    } = *facts;
    // Broad avatar tags/parent categories are compatibility context, not proof
    // that this product supplies an avatar. Description mentions never suffice.
    let base_rule = RULES.iter().find(|rule| rule.category == "Base").unwrap();
    let base_leaf = rule_match(category_name, base_rule)
        && !compatibility_context(category_name)
        && !jinxxy_addon;
    let named_original_model = (title_normalized.contains("オリジナル3dモデル")
        || title_normalized.contains("original 3d model"))
        && normalized_tags.iter().any(|tag| rule_match(tag, base_rule));
    let base_title = (affirmative_base_title(title_normalized) || named_original_model)
        && !compatibility_context(title_normalized)
        && names::has_name(title);
    if result["category"] == "Base" && !(base_leaf || base_title) {
        *result = json!({"category":"NonVRChat","basis":"fallback","inferred":true,"evidence":[]});
    }
    // BOOTH's own character category outranks tags, which also list what an avatar
    // includes: a hair style, gestures, a shader. With no specific store category,
    // a title that calls itself one named model is enough; a removed page is not.
    if store.is_none() && result["category"] != "Base" && !declared_software && !jinxxy_addon {
        if category_name == "3dキャラクター" {
            *result = json!({"category":"Base","basis":"sourceCategory","inferred":true,
                "evidence":[{"field":"sourceCategory.name","value":source_category.as_str().or_else(|| source_category["name"].as_str()).unwrap_or("")}]});
        } else if matches!(category_name, "" | "3dモデル(その他)")
            && names_one_model(title, title_normalized)
        {
            *result = json!({"category":"Base","basis":"sourceTitle","inferred":true,
                "evidence":[{"field":"sourceTitle","value":evidence_excerpt(title)}]});
        }
    }
    base_title
}

/// An explicit shader or motion tag outranks a generic texture label.
fn apply_tag_type(
    result: &mut Value,
    store: Option<(&'static str, &'static str)>,
    facts: &Facts,
    evidence: &impl Fn(&str, &Rule) -> bool,
) {
    let Facts {
        tags,
        normalized_tags,
        ..
    } = *facts;
    // A generic material/texture label must not defeat an explicit shader or
    // motion tag; a store category that says what the product is still wins.
    if store.is_none() && matches!(result["category"].as_str(), Some("Texture" | "NonVRChat")) {
        if let Some(rule) = RULES
            .iter()
            .filter(|r| matches!(r.category, "Shader" | "Motion"))
            .find(|r| normalized_tags.iter().any(|tag| evidence(tag, r)))
        {
            *result = json!({"category":rule.category,"basis":"tags","inferred":true,"evidence":tags.iter().zip(normalized_tags).filter(|(_, tag)| evidence(tag, rule)).take(8).map(|(raw, _)| json!({"field":"sourceTags","value":evidence_excerpt(raw)})).collect::<Vec<_>>()});
        }
    }
}

/// With nothing else to go on, the title, then the description, names the type.
fn apply_fallback(
    result: &mut Value,
    base_title: bool,
    facts: &Facts,
    evidence: &impl Fn(&str, &Rule) -> bool,
) {
    let Facts {
        title,
        description,
        title_normalized,
        description_normalized,
        booth,
        target_scope,
        ..
    } = *facts;
    // A description mentions dependencies, contents and what a product goes with. On
    // BOOTH, where tags and a category are there to read, it must define a shader; a
    // software definition was read above. It remains the evidence other stores have.
    if result["basis"] == "fallback" {
        for (field, content) in [
            ("sourceTitle", title_normalized),
            ("sourceDescription", description_normalized),
        ] {
            if let Some(rule) = RULES.iter().find(|rule| {
                rule.category != "NonVRChat"
                    && (rule.category != "Base" || field == "sourceTitle" && base_title)
                    && evidence(content, rule)
                    && (field == "sourceTitle" || target_scope)
                    && (field != "sourceDescription" || !booth || rule.category == "Shader")
                    && (field != "sourceDescription" || rule.category != "Motion")
                    && (field != "sourceDescription"
                        || rule.category != "Shader"
                        || description_defines_shader(content))
            }) {
                *result = json!({"category":rule.category,"basis":field,"inferred":true,"evidence":[{"field":field,"value":evidence_excerpt(if field=="sourceTitle"{title}else{description})}]});
                break;
            }
        }
    }
}

/// A prop in a generic 3D category tagged for worlds is World.
fn apply_world_prop(
    result: &mut Value,
    store: Option<(&'static str, &'static str)>,
    facts: &Facts,
) {
    let Facts {
        tags,
        normalized_tags,
        booth,
        in_3d,
        ..
    } = *facts;
    // A world prop in a generic 3D category says world and carries no avatar wording.
    if booth
        && in_3d
        && store.is_none()
        && result["category"] == "Accessory"
        && normalized_tags.iter().any(|tag| {
            tag.contains("ワールド") && !tag.contains("固定") || word_match(tag, "world")
        })
        && !normalized_tags.iter().any(|tag| {
            ["アクセサリ", "アバター", "avatar"]
                .iter()
                .any(|word| tag.contains(word))
        })
    {
        *result = json!({"category":"World","basis":"tags","inferred":true,"evidence":tags.iter().zip(normalized_tags).filter(|(_, tag)| tag.contains("ワールド") && !tag.contains("固定") || word_match(tag, "world")).take(8).map(|(raw, _)| json!({"field":"sourceTags","value":evidence_excerpt(raw)})).collect::<Vec<_>>()});
    }
}

/// Scope gates for tools and other shelves, the store fallbacks, and the scope itself.
fn apply_scope(result: &mut Value, store: Option<(&'static str, &'static str)>, facts: &Facts) {
    let Facts {
        title_normalized,
        category_name,
        parent_name,
        booth,
        jinxxy,
        gumroad,
        jinxxy_addon,
        in_3d,
        vr_scope,
        unity_scope,
        target_scope,
        explicit_vr_tag,
        explicit_vr_text,
        outside_app_context,
        outside_category,
        os_tag,
        ..
    } = *facts;
    // What a title or tag separates from BOOTH's tools still has to show where it runs.
    let store_tools =
        store.is_some() && matches!(category_name, "3dツール・システム" | "ソフトウェア");
    let restricted = store_tools || matches!(result["category"].as_str(), Some("Tool" | "Texture"));
    let store_texture = result["category"] == "Texture" && matches!(store, Some(("Texture", _)));
    let os_without_target_context = os_tag && !unity_scope && !explicit_vr_tag;
    if restricted
        && !jinxxy
        && !store_texture
        && (!target_scope || outside_app_context && !explicit_vr_tag || os_without_target_context)
    {
        *result = json!({"category":"NonVRChat","basis":"scopeGate","inferred":true,"evidence":result["evidence"]});
    }
    // Under any other top level, such as 素材データ, ゲーム or 映像作品, a word needs
    // VRChat or Unity evidence too, and a book about a VRChat world is still a book.
    let work = matches!(
        parent_name,
        "小説・その他書籍" | "漫画" | "音楽" | "音声作品" | "イラスト"
    );
    if booth
        && !parent_name.is_empty()
        && !in_3d
        && (!target_scope || work)
        && result["category"] != "NonVRChat"
    {
        *result = json!({"category":"NonVRChat","basis":"scopeGate","inferred":true,"evidence":result["evidence"]});
    }
    // The product policy treats Jinxxy as a VRChat source. Keep any specific
    // observed type; a generic asset falls under Gimmick, never the excluded shelf.
    // VRCFury alone is a scope hint, not proof that clothing is a gimmick.
    if jinxxy && result["category"] == "NonVRChat" {
        *result = if jinxxy_addon {
            json!({"category":"Gimmick","basis":"sourceType","inferred":true,
                "evidence":[{"field":"sourceCategory.types","value":"avatar_addon"}]})
        } else {
            json!({"category":"Gimmick","basis":"providerFallback","inferred":true,
                "evidence":[{"field":"provider","value":"jinxxy"}]})
        };
    }
    // Gumroad is a general-purpose store. Generic type/engine tags are not
    // evidence of a VRChat product without an explicit source title/body mention.
    let gumroad_without_vr_text = gumroad && !explicit_vr_text;
    if gumroad_without_vr_text {
        *result = json!({"category":"NonVRChat","basis":"scopeGate","inferred":true,"evidence":[]});
    }
    // BOOTH's own 3D category stands against a Photoshop or Windows tag, which often
    // says what the source files need, though not against a title naming the application.
    let store_asset = !store_tools
        && (store.is_some_and(|(kind, _)| kind != "Tool" && result["category"] == kind)
            || booth && category_name == "3dキャラクター" && result["category"] == "Base");
    let outside = if store_asset {
        strong_external_application(title_normalized) && !explicit_vr_tag
    } else {
        (outside_app_context || outside_category) && !explicit_vr_tag || os_without_target_context
    };
    result["scope"] = json!(if gumroad_without_vr_text {
        "other"
    } else if jinxxy {
        "vrchat"
    } else if outside {
        "other"
    } else if vr_scope {
        "vrchat"
    } else if unity_scope {
        "unity"
    } else if os_tag && !store_asset {
        "other"
    } else {
        "unknown"
    });
    result["scopeEvidence"] = json!({"vrchat":vr_scope,"unity":unity_scope,"explicitVrchatTag":explicit_vr_tag,"externalApplication":outside_app_context,"providerVrchat":jinxxy});
    if let Some(evidence) = result["evidence"].as_array_mut() {
        evidence.truncate(8);
        for item in evidence {
            if let Some(value) = item["value"].as_str() {
                item["value"] = json!(evidence_excerpt(value));
            }
        }
    }
}

fn names_type(text: &str, category: &str) -> bool {
    RULES
        .iter()
        .any(|rule| rule.category == category && rule_match(text, rule))
}

/// Tags saying a product goes into a world or onto an avatar, which BOOTH has no category for.
const WORLD_GIMMICK_TAGS: &[&str] = &[
    "ワールドギミック",
    "world gimmick",
    "udonギミック",
    "udon gimmick",
];
const WORLD_BUILD_TAGS: &[&str] = &["ワールドアセット", "world asset", "ワールドキット"];
const AVATAR_TAGS: &[&str] = &[
    "アバターギミック",
    "avatar gimmick",
    "modularavatar",
    "modular avatar",
    "avatar3.0",
    "avatars3.0",
    "avatar 3.0",
];

fn names_avatar_build(text: &str) -> bool {
    AVATAR_TAGS.iter().any(|word| text.contains(word)) || word_match(text, "ma対応")
}

/// What BOOTH's own category says a product is, with where that came from. Tags also
/// list what a product contains or suits, so they do not outvote it. BOOTH has no
/// category for what a product is built on, so sellers file a world's scripts, an avatar's
/// gimmicks and shaders under tools, software or worlds and say which in the title or tags.
/// Udon goes into the world SDK, Gimmick into the avatar SDK, and World is an environment
/// or a prop placed in one.
fn store_type(
    leaf: &str,
    title: &str,
    tags: &[String],
    opening: &str,
) -> Option<(&'static str, &'static str)> {
    let any_tag = |words: &[&str]| {
        tags.iter()
            .any(|tag| words.iter().any(|word| tag.contains(word)))
    };
    let system_title = word_match(title, "cvs2") || title.contains("チクワカー");
    let system_tag = any_tag(&["チクワカーシステム", "chikuwa vehicle system"]);
    let stated = match leaf {
        "3d衣装" | "3d靴" => "Clothes",
        "3d髪型" => "Hair",
        "3d装飾品" | "3d小道具" => "Accessory",
        "3dモーション・アニメーション" => "Motion",
        "3dテクスチャ" => "Texture",
        "3dツール・システム" | "ソフトウェア" => "Tool",
        "3d環境・ワールド" => "World",
        "3dキャラクター" => {
            return sells_texture(title).then_some(("Texture", "sourceTitle"))
        }
        _ if title.contains("販売ワールド") => return Some(("World", "sourceTitle")),
        _ if system_title
            || ["vrchat world prefab", "vrc world prefab"]
                .iter()
                .any(|word| title.contains(word)) =>
        {
            return Some(("Udon", "sourceTitle"))
        }
        _ if system_tag || any_tag(WORLD_GIMMICK_TAGS) => return Some(("Udon", "sourceTags")),
        _ => return None,
    };
    if stated == "Accessory" {
        return Some(
            if [
                "udonギミック",
                "udon用ギミック",
                "udon gimmick",
                "ワールドギミック",
                "worldギミック",
                "world gimmick",
            ]
            .iter()
            .any(|word| sold_as(title, word))
            {
                ("Udon", "sourceTitle")
            } else if title.contains("ワールド小物")
                || title.contains("ワールド用小物")
                || word_match(title, "world prop")
                || word_match(title, "world props")
            {
                ("World", "sourceTitle")
            } else {
                ("Accessory", "sourceCategory")
            },
        );
    }
    if !matches!(stated, "Texture" | "Tool" | "World") || title.contains("販売ワールド") {
        return Some((stated, "sourceCategory"));
    }
    tool_or_world_type(
        stated,
        leaf,
        title,
        tags,
        opening,
        system_title,
        system_tag,
        &any_tag,
    )
}

/// What a product under BOOTH's texture, tool, software or world category is: its title
/// and tags tell shaders, Udon, gimmicks and tools apart from the stated category.
fn tool_or_world_type(
    stated: &'static str,
    leaf: &str,
    title: &str,
    tags: &[String],
    opening: &str,
    system_title: bool,
    system_tag: bool,
    any_tag: &impl Fn(&[&str]) -> bool,
) -> Option<(&'static str, &'static str)> {
    let tagged = |category| tags.iter().any(|tag| names_type(tag, category));
    let world = stated == "World";
    let software_leaf = leaf == "ソフトウェア";
    // An installer that comes with a product, a stool or two rooms do not make a tool.
    let cleaned = title
        .replace("導入ツール", "")
        .replace("スツール", "")
        .replace("ツールーム", "");
    let title = cleaned.as_str();
    let sold = |words: &[&str]| words.iter().any(|word| sold_as(title, word));
    let shader = sold(&["シェーダ", "shader", "셰이더", "着色器", "著色器"]);
    // A texture made for a shader says texture too.
    if stated == "Texture" {
        return Some(if shader && !names_type(title, "Texture") {
            ("Shader", "sourceTitle")
        } else {
            ("Texture", "sourceCategory")
        });
    }
    let tool = sold(&["ツール", "工具", "도구"])
        || ["tool", "tools", "utility", "utilities", "editor extension"]
            .iter()
            .any(|word| word_match(title, word))
        || software_type(title) && !add_on_pack(title);
    // Rooms sold with their scripts are still the place, and so are trees and furniture
    // sold with their shader.
    let room = world && any_tag(&["部屋", "建物", "住宅", "ホームワールド"]);
    let scenery = world
        && (any_tag(&[
            "家具",
            "インテリア",
            "ハウス",
            "植物",
            "樹木",
            "自然",
            "背景",
            "環境",
            "小物",
        ]) || word_tag(tags, "house")
            || word_tag(tags, "prop")
            || word_tag(tags, "props"));
    let named_world = world && !tool && names_a_world(title);
    if !tool && !(named_world && (room || scenery)) && sold(&["udon"])
        || [
            "udonギミック",
            "udon用ギミック",
            "udon gimmick",
            "ワールドギミック",
            "world gimmick",
        ]
        .iter()
        .any(|word| sold_as(title, word))
        || system_title
    {
        return Some(("Udon", "sourceTitle"));
    }
    if system_tag {
        return Some(("Udon", "sourceTags"));
    }
    let gimmick = sold(&["ギミック", "gimmick", "기믹", "交互组件", "互動元件"]);
    // Named as a system or gimmick that a Udon tag explains, even under a world label.
    if !tool
        && tagged("Udon")
        && (gimmick || title.contains("システム") || word_match(title, "system"))
    {
        return Some(("Udon", "sourceTitle"));
    }
    if named_world {
        return Some(("World", "sourceCategory"));
    }
    // Particles and volumetric light are what a shader draws, whichever word a seller used.
    if !tool
        && (shader
            || world && sold(&["パーティクル", "particle"]) && !names_avatar_build(title)
            || sold(&["volumetric"]))
    {
        return Some(("Shader", "sourceTitle"));
    }
    // A tool that works on avatars names the editor; a gimmick goes on the avatar.
    let editor_tag = any_tag(&[
        "editor拡張",
        "エディタ拡張",
        "エディター拡張",
        "unityeditor",
        "unity editor",
    ]);
    // A finished world carries Udon tags for what is inside it.
    if any_tag(WORLD_GIMMICK_TAGS) && !room
        || !world
            && (any_tag(WORLD_BUILD_TAGS)
                || !tool && !editor_tag && (any_tag(&["udon"]) || word_tag(tags, "u#")))
    {
        return Some(("Udon", "sourceTags"));
    }
    if tool {
        return Some(("Tool", "sourceTitle"));
    }
    let for_worlds = [
        "ワールド用",
        "ワールド向け",
        "for world",
        "for vrchat world",
        "for vrc world",
    ]
    .iter()
    .any(|phrase| title.contains(phrase));
    if tagged("Udon") && for_worlds {
        return Some(("Udon", "sourceTitle"));
    }
    let avatar_tag = tags.iter().any(|tag| names_avatar_build(tag));
    let editing_tool = editor_tag && tagged("Tool");
    if gimmick && !editing_tool {
        let for_avatar = !for_worlds
            && ([
                "アバター用",
                "アバター向け",
                "アバターギミック",
                "for avatar",
                "avatar gimmick",
            ]
            .iter()
            .any(|word| title.contains(word))
                || avatar_tag);
        return Some((
            if (world || for_worlds) && !for_avatar {
                "Udon"
            } else {
                "Gimmick"
            },
            "sourceTitle",
        ));
    }
    if world {
        if avatar_tag {
            return Some(("Gimmick", "sourceTags"));
        }
        let drawn = ["ライト", "エフェクト", "フォグ", "レーザー", "ゴッドレイ"]
            .iter()
            .any(|word| title.contains(word) || any_tag(&[word]))
            || ["light", "effect", "fog", "laser"]
                .iter()
                .any(|word| word_match(title, word) || word_tag(tags, word));
        return Some(if tagged("Shader") && !room && !scenery && drawn {
            ("Shader", "sourceTags")
        } else {
            ("World", "sourceCategory")
        });
    }
    if names_avatar_build(title) && !editor_tag && (!tagged("Tool") || tagged("Gimmick")) {
        return Some(("Gimmick", "sourceTitle"));
    }
    let avatar_opening = ["アバターギミック", "modularavatar", "modular avatar"]
        .iter()
        .any(|word| opening.contains(word));
    if avatar_tag && !editor_tag && (!tagged("Tool") || tagged("Gimmick"))
        || tagged("Gimmick") && avatar_opening && !editor_tag
    {
        return Some(("Gimmick", "sourceTags"));
    }
    // Software lists what its output is for, so only the tool category reads the rest.
    if !software_leaf && !tagged("Tool") && !tags.iter().any(|tag| software_type(tag)) {
        if let Some(kind) = ["Udon", "Shader", "Gimmick"]
            .into_iter()
            .find(|kind| tagged(kind))
        {
            return Some((kind, "sourceTags"));
        }
    }
    Some((stated, "sourceCategory"))
}

fn word_tag(tags: &[String], word: &str) -> bool {
    tags.iter().any(|tag| word_match(tag, word))
}

/// A title calling itself a world, not a gimmick for one or something placed in one.
fn names_a_world(title: &str) -> bool {
    const NOT_A_PLACE: &[&str] = &[
        "ギミック",
        "シェーダ",
        "用",
        "向け",
        "対応",
        "専用",
        "設置",
        "制作",
        "パーツ",
        "小物",
        "アセット",
        "素材",
        "キット",
        "で",
        "に",
        "内",
        "gimmick",
        "prefab",
        "asset",
        "kit",
        "shader",
    ];
    ["ワールド", "world"].iter().any(|word| {
        title.match_indices(word).any(|(start, word)| {
            let after = &title[start + word.len()..];
            let latin_inside = word == "world"
                && (title[..start].ends_with(|c: char| c.is_ascii_alphanumeric())
                    || after.starts_with(|c: char| c.is_ascii_alphanumeric()));
            let after = after.trim_start();
            !latin_inside
                && !title[..start].contains("for ")
                && !NOT_A_PLACE.iter().any(|stop| after.starts_with(stop))
        })
    })
}

/// A title that ends by naming a texture sells one; a bonus after + or with does not.
fn sells_texture(title: &str) -> bool {
    let core = title.trim_end_matches([' ', '】', '」', '』', '）', ')', ']']);
    !title.contains('+')
        && !word_match(title, "with")
        && [
            "テクスチャ",
            "テクスチャセット",
            "テクスチャパック",
            "テクスチャ集",
            "texture",
            "textures",
            "texture set",
            "texture pack",
        ]
        .iter()
        .any(|ending| core.ends_with(ending))
}

/// A word for what is sold, not for something the product comes with or suits.
fn sold_as(title: &str, word: &str) -> bool {
    title.match_indices(word).any(|(start, word)| {
        let after = title[start + word.len()..]
            .trim_start_matches('ー')
            .trim_start();
        let after = after.strip_prefix("ギミック").unwrap_or(after);
        !title[..start].ends_with("with ")
            && ![
                "付",
                "つき",
                "搭載",
                "あり",
                "有り",
                "多数",
                "満載",
                "入り",
                "同梱",
                "対応",
                "専用",
                "用",
                "向け",
                "込み",
                "使用",
                "不使用",
                "なし",
                "無し",
                "非対応",
            ]
            .iter()
            .any(|suffix| after.starts_with(suffix))
    })
}

/// Add-on wording that sells one more pack rather than naming an application.
fn add_on_pack(title: &str) -> bool {
    let rest = ["add-on", "addon", "add on", "アドオン", "애드온"]
        .iter()
        .fold(title.to_owned(), |rest, word| rest.replace(word, ""));
    !software_type(&rest) && !title.contains("blender")
}

/// Words that name motion data. モーショングラフィックス is video work.
fn names_motion(text: &str) -> bool {
    let text = text
        .replace("モーショングラフィック", "")
        .replace("motion graphic", "");
    [
        "モーション",
        "모션",
        "动作",
        "アニメーションファイル",
        "アニメーションクリップ",
    ]
    .iter()
    .any(|word| text.contains(word))
        // 動作 usually says that something works.
        || text == "動作"
        || [
            "motion",
            "motions",
            "gesture",
            "gestures",
            "pose",
            "poses",
            "animationclip",
            "animation clip",
        ]
        .iter()
        .any(|word| word_match(&text, word))
}

/// These also sell chat emotes and drawing references, so they need context.
fn names_emote_or_pose(text: &str) -> bool {
    text.contains("エモート")
        || word_match(text, "emote")
        || word_match(text, "emotes")
        || text
            .replace("プロポーズ", "")
            .replace("コンポーズ", "")
            .contains("ポーズ")
}

fn bare_animation(text: &str) -> bool {
    text.contains("アニメーション")
        || text.contains("애니메이션")
        || word_match(text, "animation")
        || word_match(text, "animations")
}

fn mentions_video_or_effect(text: &str) -> bool {
    ["エフェクト", "パーティクル", "動画", "映像", "動く背景"]
        .iter()
        .any(|word| text.contains(word))
        || [
            "effect",
            "effects",
            "particle",
            "particles",
            "vfx",
            "gif",
            "vj",
        ]
        .iter()
        .any(|word| word_match(text, word))
}

// Keep runtime/platform scope separate from product type. Blender alone does
// not imply either software or VRChat; an avatar may include a .blend source.
fn software_type(text: &str) -> bool {
    [
        "software",
        "add-on",
        "addon",
        "add on",
        "plugin",
        "plug-in",
        "editor extension",
    ]
    .iter()
    .any(|word| word_match(text, word))
        || [
            "ソフトウェア",
            "アドオン",
            "プラグイン",
            "エディタ拡張",
            "エディター拡張",
            "软件",
            "軟體",
            "编辑器扩展",
            "編輯器擴充",
            "애드온",
            "플러그인",
        ]
        .iter()
        .any(|word| text.contains(word))
}

fn description_defines_software(opening: &str) -> bool {
    if ["requires", "required", "dependency", "必要", "依存"]
        .iter()
        .any(|word| opening.contains(word))
    {
        return false;
    }
    [
        "unity editor tool",
        "unity editor extension",
        "blender addon",
        "blender add-on",
        "アドオンです",
        "ツールです",
        "エディター専用ツール",
        "エディタ専用ツール",
    ]
    .iter()
    .any(|word| opening.contains(word))
        || opening.contains("유니티 에디터") && opening.contains("툴")
}

fn affirmative_base_title(title: &str) -> bool {
    let types = [
        "original 3d avatar",
        "original avatar",
        "base avatar",
        "3d avatar",
        "3d character",
        "vrchat avatar",
    ];
    if types.iter().any(|kind| word_match(title, kind))
        || [
            "オリジナル3dアバター",
            "オリジナルアバター",
            "3dアバター",
            "3dキャラクター",
        ]
        .iter()
        .any(|kind| title.contains(kind))
    {
        return true;
    }
    // A generic avatar word qualifies only when it directly introduces one
    // quoted name and consumes the whole remaining title; suffix prose stays.
    for prefix in ["アバター", "avatar"] {
        if let Some(rest) = title.strip_prefix(prefix).map(str::trim) {
            if rest.starts_with(['「', '『', '"', '“']) {
                let clean = names::cleaned_title(rest);
                if clean != rest
                    && names::has_name(&clean)
                    && !clean.contains(['「', '」', '『', '』', '"', '“', '”'])
                {
                    return true;
                }
            }
        }
    }
    false
}

/// How sellers call one 3D model. Compared with spaces removed, because they also
/// write オリジナル 3Dモデル and VRChat対応 3Dモデル.
fn model_wording() -> impl Iterator<Item = String> {
    let platform = ["vrchat", "vrc"].into_iter().flat_map(|platform| {
        ["向け", "用", "対応"].into_iter().flat_map(move |purpose| {
            ["3dモデル", "モデル", "アバター"]
                .into_iter()
                .map(move |kind| format!("{platform}{purpose}{kind}"))
        })
    });
    [
        "オリジナル3dモデル".to_owned(),
        "original3dmodel".to_owned(),
    ]
    .into_iter()
    .chain(platform)
}

/// The wording must introduce the name or directly follow a quoted one, because a
/// garment can end with the same phrase. Product wording after + or & can be a bonus
/// that comes with the model, so there only the name parser decides.
fn names_one_model(title: &str, normalized: &str) -> bool {
    let compact: String = normalized.split_whitespace().collect();
    let product = normalized.split(['+', '&']).next().unwrap_or(normalized);
    model_wording().any(|word| {
        compact.match_indices(&word).any(|(start, _)| {
            let after = &compact[start + word.len()..];
            // "original 3d modeler" is not the phrase.
            !(word.is_ascii() && after.starts_with(|c: char| c.is_ascii_alphanumeric()))
                && !names::for_target(after)
                && (after.chars().any(char::is_alphanumeric)
                    || compact[..start]
                        .trim_end_matches('【')
                        .ends_with(['」', '』', '”', '"', '〛']))
        })
    }) && !compatibility_context(normalized)
        && !names::addon(product)
        && !RULES
            .iter()
            .any(|rule| rule.specific && rule_match(product, rule))
        && names::product_base_parts(title).name_kind != "fallback"
}

fn compatibility_context(text: &str) -> bool {
    // Complete known platform qualifiers (e.g. "for VRChat") describe the
    // target platform, whereas "for <avatar name>" describes compatibility.
    let cleaned = names::cleaned_title(text);
    let text = cleaned.as_str();
    [
        "for",
        "compatible",
        "compatibility",
        "support",
        "supports",
        "supported",
    ]
    .iter()
    .any(|word| word_match(text, word))
        || [
            "アバター対応",
            "アバター用",
            "アバター向け",
            "アバター専用",
            "対応アバター",
            "対応モデル",
            "素体用",
            "素體用",
        ]
        .iter()
        .any(|word| text.contains(word))
}

fn strong_external_application(text: &str) -> bool {
    [
        "aftereffects",
        "after effects",
        "aftereffcts",
        "ae",
        "novelai",
        "photoshop",
        "clip studio",
        "clipstudiopaint",
        "live2d",
        "ccfolia",
        "ココフォリア",
        "trpg",
        "coc",
    ]
    .iter()
    .any(|term| word_match(text, term))
}

fn description_defines_shader(description: &str) -> bool {
    let opening: String = description.chars().take(180).collect();
    ![
        "requires",
        "required",
        "dependency",
        "依存",
        "必要",
        "使用シェーダ",
    ]
    .iter()
    .any(|word| opening.contains(word))
        && [
            "unity shader",
            "ugui shader",
            "shader for",
            "shader package",
            "シェーダーです",
            "シェーダです",
            "シェーダーを作成",
            "シェーダを作成",
        ]
        .iter()
        .any(|word| opening.contains(word))
}

struct Avatar {
    value: Value,
    names: Vec<(String, String)>,
}

fn avatars() -> &'static Vec<Avatar> {
    static CACHE: OnceLock<Vec<Avatar>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let entries: Vec<Value> = serde_json::from_str(include_str!("avatars.json")).expect("bundled avatar dictionary must be valid JSON");
        entries.into_iter().map(|mut entry| {
            let official_name = entry["officialName"].as_str().expect("officialName").to_owned();
            let mut names = vec![(normalize(&official_name), official_name)];
            names.extend(entry["aliases"].as_array().expect("aliases").iter().filter_map(Value::as_str).map(|name| (normalize(name), name.to_owned())));
            if let Some(name) = entry["englishName"].as_str() {
                names.push((normalize(name), name.to_owned()));
            }
            names.sort();
            names.dedup();
            entry["id"] = json!(format!("booth:{}", entry["productId"].as_str().expect("productId")));
            entry["provenance"] = json!({"kind":"officialPublicListing", "url":entry["sourceUrl"],
                "observedAt":entry["englishNameEvidence"]["observedAt"].as_str().unwrap_or("2026-10-09"), "dictionaryVersion":VERSION,
                "fields":["productId","officialName","creator"], "aliasPolicy":"see taxonomy/README.md"});
            entry["displayName"] = json!(avatar_display_name(&entry));
            Avatar { value: entry, names }
        }).collect()
    })
}

fn stripped_avatar_name(text: &str) -> String {
    names::cleaned_title(text)
}

struct ExactAvatarLookup {
    names: BTreeMap<String, usize>,
    pairs: BTreeMap<String, usize>,
}

fn exact_avatar_lookup() -> &'static ExactAvatarLookup {
    static LOOKUP: OnceLock<ExactAvatarLookup> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        let mut lookup = ExactAvatarLookup {
            names: BTreeMap::new(),
            pairs: BTreeMap::new(),
        };
        for (index, avatar) in avatars().iter().enumerate() {
            for (name, _) in &avatar.names {
                lookup.names.entry(name.clone()).or_insert(index);
            }
            for (left, _) in &avatar.names {
                for (right, _) in &avatar.names {
                    for separator in [" / ", "/", " ", " -", " - ", "-", " (", "(", " 「", "『"] {
                        lookup
                            .pairs
                            .entry(format!("{left}{separator}{right}").trim().to_owned())
                            .or_insert(index);
                    }
                }
            }
        }
        lookup
    })
}

/// Input must already be normalized. Preserve the prior first-dictionary-entry
/// precedence even if a future direct alias collides with another entry's pair.
fn exact_avatar_index_normalized(name: &str) -> Option<usize> {
    let lookup = exact_avatar_lookup();
    let pair_key = name.trim_end_matches(['-', ')', '」', '』']).trim();
    lookup
        .names
        .get(name)
        .into_iter()
        .chain(lookup.pairs.get(pair_key))
        .copied()
        .min()
}

fn exact_avatar_normalized(name: &str) -> Option<&'static Avatar> {
    exact_avatar_index_normalized(name).map(|index| &avatars()[index])
}

fn avatar_tag_normalized(normalized: &str) -> Option<&'static Avatar> {
    let bare = ["対応", "専用", "用", "對應", "对应", "向け"]
        .iter()
        .find_map(|suffix| normalized.strip_suffix(suffix))
        .unwrap_or(normalized)
        .trim();
    // Resolve full/bare candidates in dictionary order, matching the old loop.
    let full = exact_avatar_index_normalized(normalized);
    if bare == normalized {
        return full.map(|index| &avatars()[index]);
    }
    let bare_index = exact_avatar_index_normalized(bare).or_else(|| {
        // A compatibility suffix can sit outside a complete name wrapper.
        // The cleaned whole name must still be an exact known alias.
        exact_avatar_index_normalized(&normalize(&stripped_avatar_name(bare)))
    });
    full.into_iter()
        .chain(bare_index)
        .min()
        .map(|index| &avatars()[index])
}

/// Exact canonical avatar tags are compatibility evidence, never file-access
/// or purchase evidence. Unlike match_avatars this rejects apparel substrings.
pub fn exact_avatar_tag(text: &str) -> Option<Value> {
    avatar_tag_normalized(&normalize(&stripped_avatar_name(text))).map(|avatar| {
        let mut candidate = avatar.value.clone();
        candidate["basis"] = json!("exactTag");
        candidate["matchedAliases"] = json!([text]);
        candidate["entitlementEstablished"] = json!(false);
        candidate
    })
}

/// Remove recognized model marketing wrappers, not arbitrary avatar mentions in
/// apparel names. Cleaning is presentation only and never establishes identity.
pub fn clean_avatar_name(text: &str) -> String {
    let name = stripped_avatar_name(text);
    exact_avatar_normalized(&normalize(&name))
        .and_then(|avatar| avatar.value["officialName"].as_str())
        .unwrap_or(&name)
        .to_owned()
}

/// Compact Base-label presentation. Kind describes parsing certainty, never
/// product identity, entitlement or automatic enablement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarLabelParts {
    pub original: String,
    pub source_latin: Option<String>,
    pub qualifiers: Vec<String>,
    pub name_kind: &'static str,
}

pub fn parsed_avatar_label(title: &str) -> AvatarLabelParts {
    names::structured_base_parts(title)
}

/// A special edition or another listing of one avatar leaves its plain listing's
/// name unambiguous.
pub fn avatar_edition(entry: &Value) -> bool {
    entry["variantListing"] == true
        || entry["labelQualifiers"]
            .as_array()
            .is_some_and(|values| !values.is_empty())
}

/// The name located in a known Base product's title, plus bounded presentation
/// corrections tied to the public product identity. Similar titles and user
/// renames never inherit another product's name.
pub fn parsed_product_avatar_label(product_id: &str, title: &str) -> AvatarLabelParts {
    let mut parsed = names::product_base_parts(title);
    let corrected = match product_id.strip_prefix("booth:").unwrap_or(product_id) {
        // https://booth.pm/ja/items/8547624 — name stem only; Latin unverified.
        "8547624" if title.contains("メイネーラ") => Some("メイネーラ"),
        // https://booth.pm/en/items/4309253 — franchise prefix is not the name.
        "4309253" if title.contains("棗") && title.contains("イロハ") => Some("棗 イロハ"),
        _ => None,
    };
    if let Some(original) = corrected {
        parsed.original = original.to_owned();
        parsed.source_latin = None;
        parsed.name_kind = "single";
    }
    parsed
}

/// Conservative legacy label parsing used by exact identity/alias paths.
pub fn avatar_label_parts(title: &str) -> (String, Option<String>) {
    let (original, english) = names::base_parts(title);
    (
        clean_avatar_name(if original.trim().is_empty() {
            title
        } else {
            &original
        }),
        english,
    )
}

pub fn avatar_label_is_unambiguous(title: &str) -> bool {
    names::has_name(title)
}

/// Kana romanization with a cached local Windows phonetic fallback for kanji.
/// OS readings are fallback spellings, never authoritative English names.
pub fn romanized_name(input: &str) -> String {
    let clean = stripped_avatar_name(input);
    if let Some(avatar) = exact_avatar_normalized(&normalize(&clean)) {
        return avatar.value["englishName"]
            .as_str()
            .unwrap_or("")
            .to_owned();
    }
    let romanized = romanize_kana(&clean);
    if !romanized.is_empty() {
        return romanized;
    }
    phonetic::reading(&reading_input(&clean))
        .map(|reading| romanize_kana(&reading))
        .unwrap_or_default()
}
// Typography is not a missing pronunciation. Preserve letters (including
// unknown Han) so unsupported readings cannot silently become partial names.
// Strip decoration before the OS reader, which otherwise pronounces ★ as hoshi.
fn reading_input(input: &str) -> String {
    let mut result = String::new();
    for c in input.nfkc() {
        match c {
            '‐' | '‑' | '‒' | '–' | '—' | '―' | '−' => result.push('-'),
            '’' | '‘' | 'ʼ' => result.push('\''),
            c if c.is_alphanumeric()
                || matches!(
                    c,
                    'ー' | '々' | 'ゝ' | 'ゞ' | 'ヽ' | 'ヾ' | '\u{3099}' | '\u{309a}'
                ) =>
            {
                result.push(c)
            }
            c if c.is_ascii() && matches!(c, '-' | '\'' | '_' | '.' | '+' | '&') => result.push(c),
            '\u{0300}'..='\u{036f}' | '\u{fe00}'..='\u{fe0f}' => {}
            _ => result.push(' '),
        }
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn romanize_kana(clean: &str) -> String {
    let prepared = reading_input(clean);
    let kana: Vec<char> = prepared
        .nfkc()
        .map(|c| {
            if ('ァ'..='ヶ').contains(&c) {
                char::from_u32(c as u32 - 0x60).unwrap()
            } else if ('ㇰ'..='ㇿ').contains(&c) {
                [
                    'く', 'し', 'す', 'と', 'ぬ', 'は', 'ひ', 'ふ', 'へ', 'ほ', 'む', 'ら', 'り',
                    'る', 'れ', 'ろ',
                ][(c as u32 - 0x31f0) as usize]
            } else {
                c
            }
        })
        .collect();
    let mut result = String::new();
    let mut index = 0;
    let mut double = false;
    while index < kana.len() {
        let c = kana[index];
        if c == 'っ' {
            double = true;
            index += 1;
            continue;
        }
        if c == 'ー' {
            if let Some(vowel) = result.chars().rev().find(|c| "aeiou".contains(*c)) {
                result.push(vowel);
            }
            index += 1;
            continue;
        }
        let base = match c {
            'あ' | 'ぁ' => "a",
            'い' | 'ぃ' => "i",
            'う' | 'ぅ' => "u",
            'え' | 'ぇ' => "e",
            'お' | 'ぉ' => "o",
            'か' => "ka",
            'き' => "ki",
            'く' => "ku",
            'け' => "ke",
            'こ' => "ko",
            'が' => "ga",
            'ぎ' => "gi",
            'ぐ' => "gu",
            'げ' => "ge",
            'ご' => "go",
            'さ' => "sa",
            'し' => "shi",
            'す' => "su",
            'せ' => "se",
            'そ' => "so",
            'ざ' => "za",
            'じ' => "ji",
            'ず' => "zu",
            'ぜ' => "ze",
            'ぞ' => "zo",
            'た' => "ta",
            'ち' => "chi",
            'つ' => "tsu",
            'て' => "te",
            'と' => "to",
            'だ' => "da",
            'ぢ' => "ji",
            'づ' => "zu",
            'で' => "de",
            'ど' => "do",
            'な' => "na",
            'に' => "ni",
            'ぬ' => "nu",
            'ね' => "ne",
            'の' => "no",
            'は' => "ha",
            'ひ' => "hi",
            'ふ' => "fu",
            'へ' => "he",
            'ほ' => "ho",
            'ば' => "ba",
            'び' => "bi",
            'ぶ' => "bu",
            'べ' => "be",
            'ぼ' => "bo",
            'ぱ' => "pa",
            'ぴ' => "pi",
            'ぷ' => "pu",
            'ぺ' => "pe",
            'ぽ' => "po",
            'ま' => "ma",
            'み' => "mi",
            'む' => "mu",
            'め' => "me",
            'も' => "mo",
            'や' | 'ゃ' => "ya",
            'ゆ' | 'ゅ' => "yu",
            'よ' | 'ょ' => "yo",
            'ら' => "ra",
            'り' => "ri",
            'る' => "ru",
            'れ' => "re",
            'ろ' => "ro",
            'わ' | 'ゎ' => "wa",
            'ゐ' => "i",
            'ゑ' => "e",
            'ゕ' => "ka",
            'ゖ' => "ke",
            'を' => "wo",
            'ん' => "n",
            'ゔ' => "vu",
            _ => {
                if c.is_ascii() {
                    result.push(c);
                    index += 1;
                    continue;
                } else if matches!(c, 'ゝ' | 'ゞ') && index > 0 {
                    let mut repeated = kana[index - 1].to_string();
                    if c == 'ゞ' {
                        repeated.push('\u{3099}');
                    }
                    let syllable = romanize_kana(&repeated).to_ascii_lowercase();
                    if syllable.is_empty() {
                        return String::new();
                    }
                    result.push_str(&syllable);
                    index += 1;
                    continue;
                } else if matches!(c as u32, 0x00c0..=0x024f) {
                    let latin: String = c
                        .to_string()
                        .nfkd()
                        .filter(|c| c.is_ascii_alphabetic())
                        .collect();
                    if latin.is_empty() {
                        return String::new();
                    }
                    result.push_str(&latin);
                    index += 1;
                    continue;
                } else {
                    return String::new();
                }
            }
        };
        let mut syllable = base.to_owned();
        if let Some(small) = kana
            .get(index + 1)
            .filter(|c| matches!(c, 'ゃ' | 'ゅ' | 'ょ'))
        {
            if let Some(without_i) = base.strip_suffix('i') {
                let stem = match base {
                    "shi" => "sh",
                    "chi" => "ch",
                    "ji" => "j",
                    _ => without_i,
                };
                syllable = format!(
                    "{}{}{}",
                    stem,
                    if matches!(base, "shi" | "chi" | "ji") {
                        ""
                    } else {
                        "y"
                    },
                    match small {
                        'ゃ' => "a",
                        'ゅ' => "u",
                        _ => "o",
                    }
                );
                index += 1;
            }
        }
        if double {
            if let Some(first) = syllable.chars().next().filter(|c| !"aeioun".contains(*c)) {
                result.push(first);
            }
            double = false;
        }
        result.push_str(&syllable);
        index += 1;
    }
    let mut result = result.split_whitespace().collect::<Vec<_>>().join(" ");
    if !result.chars().any(|c| c.is_ascii_alphanumeric()) {
        return String::new();
    }
    if let Some(index) = result.find(|c: char| c.is_ascii_alphabetic()) {
        result[index..index + 1].make_ascii_uppercase();
    }
    result
}

pub fn avatar_display_name(avatar: &Value) -> String {
    let original = avatar["officialName"].as_str().unwrap_or("");
    let english = avatar["englishName"].as_str().unwrap_or("");
    let mut display = if english.is_empty() || normalize(original) == normalize(english) {
        original.to_owned()
    } else {
        format!("{original} / {english}")
    };
    let qualifiers: Vec<_> = avatar["labelQualifiers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    if !qualifiers.is_empty() {
        display.push_str(&format!(" ({})", qualifiers.join(", ")));
    }
    display
}

// Conservative boundaries keep short names such as 萌 out of unrelated words.
// Compatibility suffixes are only lexical evidence, never ownership evidence.
fn name_match(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    text.match_indices(name).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = &text[start + name.len()..];
        let first = name.chars().next().unwrap();
        let left_ok =
            !before.is_some_and(|c| c.is_alphanumeric() && (c.is_ascii() == first.is_ascii()));
        let last = name.chars().next_back().unwrap();
        let right_ok = !after
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() && (c.is_ascii() == last.is_ascii()))
            || ["対応", "専用", "用", "對應", "对应", "向け", "ちゃん"]
                .iter()
                .any(|suffix| after.starts_with(suffix));
        left_ok && right_ok
    })
}

/// Candidate name mentions. A result is never a product-ID resolver and must not
/// be promoted into ownedAvatars without independent account/file/user evidence.
pub fn match_avatars(text: &str) -> Vec<Value> {
    let text = normalize(text);
    avatars()
        .iter()
        .filter_map(|avatar| {
            let matched: Vec<_> = avatar
                .names
                .iter()
                .filter(|(key, _)| name_match(&text, key))
                .map(|(_, name)| name.clone())
                .collect();
            if matched.is_empty() {
                return None;
            }
            let mut candidate = avatar.value.clone();
            candidate["matchedAliases"] = json!(matched);
            candidate["basis"] = json!("nameMention");
            candidate["entitlementEstablished"] = json!(false);
            Some(candidate)
        })
        .collect()
}

/// Normalized index expansion, including the normalized input itself. Do not
/// AND all these values as query terms: they are alternate searchable spellings.
pub fn aliases(text: &str) -> Vec<String> {
    let mut values = BTreeSet::new();
    let text = normalize(text);
    if !text.is_empty() {
        values.insert(text.clone());
    }
    for avatar in avatars() {
        if avatar.names.iter().any(|(key, _)| name_match(&text, key)) {
            values.extend(avatar.names.iter().map(|(key, _)| key.clone()));
        }
    }
    for tag in TAGS {
        let names: Vec<_> = std::iter::once(tag.label)
            .chain(tag.aliases.iter().copied())
            .map(normalize)
            .collect();
        if names.iter().any(|name| name_match(&text, name)) {
            values.extend(names);
        }
    }
    values.into_iter().collect()
}

/// Exact tag equivalence (including a bare compatibility suffix). Unlike a
/// text mention matcher this cannot turn "Mafuyu jacket" into a whole-avatar tag.
pub fn canonical_tag(text: &str) -> String {
    let normalized = normalize(&stripped_avatar_name(text));
    if let Some(avatar) = avatar_tag_normalized(&normalized) {
        return avatar.value["id"].as_str().unwrap().to_owned();
    }
    for tag in TAGS {
        if normalize(tag.label) == normalized
            || tag
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized)
        {
            return tag.id.to_owned();
        }
    }
    normalized
}

/// Caller keeps the returned raw spelling and owns durable overlay/rejection
/// persistence. This only de-duplicates equivalents and applies explicit rejection.
#[cfg(test)]
pub fn effective_tags(source: &[String], user: &[String], rejected: &[String]) -> Vec<String> {
    let rejected: BTreeSet<_> = rejected.iter().map(|tag| canonical_tag(tag)).collect();
    let mut seen = BTreeSet::new();
    user.iter()
        .chain(source)
        .filter(|tag| !tag.trim().is_empty())
        .filter(|tag| {
            let key = canonical_tag(tag);
            !rejected.contains(&key) && seen.insert(key)
        })
        .cloned()
        .collect()
}

/// Library Base records supplement choices by explicit product identity only.
/// The supplied records must be effective products already in the catalogue.
/// Classification and presence in that catalogue do not establish file access.
pub fn owned_base_options(products: &[Value]) -> Vec<Value> {
    let mut options = BTreeMap::<String, Value>::new();
    for product in products {
        if product["category"].as_str() != Some("Base") {
            continue;
        }
        let Some(id) = product["id"].as_str().filter(|id| !id.trim().is_empty()) else {
            continue;
        };
        let known = avatars()
            .iter()
            .find(|avatar| avatar.value["productId"].as_str() == Some(id));
        let source_title = product["sourceTitle"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                product["title"]
                    .as_str()
                    .filter(|name| !name.trim().is_empty())
            });
        let title = source_title.unwrap_or(id);
        let mut entry = known.map(|avatar| avatar.value.clone()).unwrap_or_else(|| {
            json!({
                "id":format!("product:{id}"), "productId":id, "officialName":title,
                "aliases":[], "provenance":{"kind":"catalogueBaseProduct", "productId":id},
            })
        });
        if known.is_none() {
            let parsed = if source_title.is_some() {
                parsed_product_avatar_label(id, title)
            } else {
                AvatarLabelParts {
                    original: id.to_owned(),
                    source_latin: None,
                    qualifiers: vec![],
                    name_kind: "fallback",
                }
            };
            let english = if parsed.name_kind == "single" {
                parsed
                    .source_latin
                    .clone()
                    .unwrap_or_else(|| romanized_name(&parsed.original))
            } else {
                String::new()
            };
            entry["officialName"] = json!(parsed.original);
            entry["nameKind"] = json!(parsed.name_kind);
            entry["labelQualifiers"] = json!(parsed.qualifiers);
            entry["variantListing"] = json!(names::variant_listing(title, &parsed.original));
            entry["englishNameStatus"] = json!(if english.is_empty() {
                "unresolvedReading"
            } else if parsed.source_latin.is_some() {
                "sourceTitleSpelling"
            } else {
                "romanizationFallback"
            });
            entry["englishName"] = json!(english);
            entry["englishNameEvidence"] = json!({"kind":if parsed.source_latin.is_some(){"sourceTitleSpelling"}else{"presentationFallback"},"identityEstablished":false});
        } else {
            entry["nameKind"] = json!("single");
            entry["labelQualifiers"] = json!([]);
        }
        entry["displayName"] = json!(avatar_display_name(&entry));
        entry["basis"] = json!("catalogueBaseProduct");
        entry["entitlementEstablished"] = json!(false);
        entry["ownedProductIds"] = json!([id]);
        options.insert(entry["id"].as_str().unwrap().to_owned(), entry);
    }
    options.into_values().collect()
}

struct Tag {
    id: &'static str,
    label: &'static str,
    kind: &'static str,
    aliases: &'static [&'static str],
}
// Original ordinary-language vocabulary, not imported from any tag/rule table.
const TAGS: &[Tag] = &[
    Tag {
        id: "style:casual",
        label: "Casual",
        kind: "style",
        aliases: &["カジュアル", "休闲", "休閒", "캐주얼"],
    },
    Tag {
        id: "style:formal",
        label: "Formal",
        kind: "style",
        aliases: &["フォーマル", "正装", "正裝", "포멀"],
    },
    Tag {
        id: "style:gothic",
        label: "Gothic",
        kind: "style",
        aliases: &["ゴシック", "哥特", "哥德", "고딕"],
    },
    Tag {
        id: "style:fantasy",
        label: "Fantasy",
        kind: "style",
        aliases: &["ファンタジー", "奇幻", "판타지"],
    },
    Tag {
        id: "style:cyberpunk",
        label: "Cyberpunk",
        kind: "style",
        aliases: &["サイバーパンク", "赛博朋克", "賽博龐克", "사이버펑크"],
    },
    Tag {
        id: "use:vrchat",
        label: "VRChat",
        kind: "use",
        aliases: &["vrc"],
    },
    Tag {
        id: "use:vrm",
        label: "VRM",
        kind: "use",
        aliases: &[],
    },
    Tag {
        id: "use:streaming",
        label: "Streaming",
        kind: "use",
        aliases: &["配信", "直播", "스트리밍"],
    },
    Tag {
        id: "rating:r18",
        label: "R-18",
        kind: "rating",
        aliases: &["r18", "成人向け", "成人向", "성인용"],
    },
    Tag {
        id: "rating:all-ages",
        label: "All ages",
        kind: "rating",
        aliases: &["全年齢", "全年龄", "全年齡", "전연령"],
    },
];

/// Suggestions retain uncertainty; rating suggestions never set sensitive=true.
pub fn tag_suggestions(text: &str, rejected: &[String]) -> Vec<Value> {
    let normalized = normalize(text);
    let rejected: BTreeSet<_> = rejected.iter().map(|tag| canonical_tag(tag)).collect();
    TAGS.iter().filter(|tag| !rejected.contains(tag.id)).filter(|tag| {
        std::iter::once(tag.label).chain(tag.aliases.iter().copied()).any(|alias| name_match(&normalized, &normalize(alias)))
    }).map(|tag| json!({"id":tag.id,"label":tag.label,"kind":tag.kind,"basis":"nameMention","inferred":true})).collect()
}

pub fn dictionary() -> Value {
    json!({"version":VERSION, "categories":CATEGORY_KEYS,
        "avatars":avatars().iter().map(|avatar| avatar.value.clone()).collect::<Vec<_>>(),
        "tags":TAGS.iter().map(|tag| json!({"id":tag.id,"label":tag.label,"kind":tag.kind,"aliases":tag.aliases,"provenance":{"kind":"originalVocabulary","version":VERSION}})).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests;
