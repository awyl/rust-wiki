// Retrieval benchmark fixture (Phase 1, lexical).
//
// 20 sanitized pages + 60 graded queries, mirroring
// zosmaai/pi-llm-wiki test/fixtures/retrieval-benchmark/fixture.ts.
// Pages model rust-wiki's own topics; bodies carry discriminating terms in
// the first ~200 chars (that window is what the registry excerpt indexes).
// No raw home paths, emails, credentials, or copied private notes.

pub const BENCHMARK_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Canonical,
    Evidence,
}

#[derive(Debug, Clone)]
pub struct Judgment {
    pub page_id: &'static str,
    pub grade: u8, // 3 = direct answer, 2 = supporting, 1 = context
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Train,
    Heldout,
}

#[derive(Debug, Clone, Copy)]
pub enum Category {
    ExactLookup,
    EntityAlias,
    Paraphrase,
    VagueRecollection,
    Conceptual,
    GraphScope,
    EvidenceRequest,
    Temporal,
    Contradiction,
    Conclusion,
    Synthesis,
    Negative,
}

impl Category {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ExactLookup => "exact_lookup",
            Self::EntityAlias => "entity_alias",
            Self::Paraphrase => "paraphrase",
            Self::VagueRecollection => "vague_recollection",
            Self::Conceptual => "conceptual",
            Self::GraphScope => "graph_scope",
            Self::EvidenceRequest => "evidence_request",
            Self::Temporal => "temporal",
            Self::Contradiction => "contradiction",
            Self::Conclusion => "conclusion",
            Self::Synthesis => "synthesis",
            Self::Negative => "negative",
        }
    }
}

pub struct Page {
    pub id: &'static str,
    pub markdown: &'static str,
}

pub struct Query {
    pub id: &'static str,
    pub text: &'static str,
    pub category: Category,
    pub split: Split,
    pub judgments: &'static [Judgment],
    /// Contradiction queries: pages whose claims must all appear in the top set.
    pub expected_conflicts: &'static [&'static str],
}

const fn j(page_id: &'static str, grade: u8, role: Role) -> Judgment {
    Judgment { page_id, grade, role }
}

pub const PAGES: &[Page] = &[
    Page {
        id: "entities/nomic-embed-text-v1-5",
        markdown: "---\ntype: entity\ntitle: Nomic Embed Text v1.5\ncategory: tool\ncreated: 2026-09-10\n---\n\n# Nomic Embed Text v1.5\n\nThe embedding model served through aiproxy: 768 dimensions, called with the embeddings-local prefix. Cold start around 54 seconds, warm calls take milliseconds. Vectors index chunks, not whole pages.\n",
    },
    Page {
        id: "entities/rust-wiki",
        markdown: "---\ntype: entity\ntitle: rust-wiki\ncategory: tool\ncreated: 2026-09-06\n---\n\n# rust-wiki\n\nA remote zosmaai-style wiki MCP server written in Rust. It is the mechanical storage and engine layer: vaults, registry, backlinks, lint, recall, git backing. Synthesis and synthesis judgment stay with the calling agent.\n",
    },
    Page {
        id: "concepts/karpathy-pattern",
        markdown: "---\ntype: concept\ntitle: Karpathy LLM Wiki Pattern\nrelevance: critical\ncreated: 2026-09-06\n---\n\n# Karpathy LLM Wiki Pattern\n\nThe original idea that an LLM should write its own markdown wiki as persistent memory. One file per thing, kebab cases, frontmatter, cross-links, and raw sources read only. Agents read it at task start and write insights at task end.\n",
    },
    Page {
        id: "concepts/okf-v0-2",
        markdown: "---\ntype: concept\ntitle: OKF v0.2\ncreated: 2026-09-09\n---\n\n# OKF v0.2\n\nThe open knowledge format this vault follows: a machine-generated, write-protected layout of wiki, meta, and raw directories plus an okf_version marker in the root index. Backward-compatible reading of legacy vaults needs a migration step.\n",
    },
    Page {
        id: "concepts/embedding-store",
        markdown: "---\ntype: concept\ntitle: Embedding Store\ncreated: 2026-09-10\n---\n\n# Embedding Store\n\nmeta/embeddings.json keeps one list of vectors per chunk per page, plus a content hash of the chunk text. Reindex skips pages whose hash is unchanged, so editing one page re-embeds only that page. The store format changed to chunks, which broke old-format reads.\n",
    },
    Page {
        id: "concepts/semantic-candidate-admission",
        markdown: "---\ntype: concept\ntitle: Semantic Candidate Admission\ncreated: 2026-09-10\n---\n\n# Semantic Candidate Admission\n\nAdditive fusion: pages the lexical pass found get a semantic boost, and pages lexical search missed are admitted when their best-chunk cosine clears the 0.2 floor, scored on the semantic term alone. This lets a paraphrase surfaced by vectors still reach the top results even with no lexical overlap. Semantic candidates can therefore appear for non-english phrasing as well.\n",
    },
    Page {
        id: "concepts/wikilink-gate",
        markdown: "---\ntype: concept\ntitle: Wikilink Gate\ncreated: 2026-09-11\n---\n\n# Wikilink Gate\n\nA pre-write gate over body links with off, validate, and normalize modes. Bare double-bracket wikilinks are canonicalized to markdown links. Code spans and fenced blocks pass through verbatim, so a page documenting link syntax never has its own samples rewritten into live links.\n",
    },
    Page {
        id: "concepts/relevance-claim",
        markdown: "---\ntype: concept\ntitle: Relevance Claim\ncreated: 2026-09-11\n---\n\n# Relevance Claim\n\nAn optional frontmatter claim of low, medium, high, or critical. Recall multiplies the page score by 0.9, 1.0, 1.1, or 1.2, so a comparable match settles in favor of the page that states importance. It is bounded on purpose: a high claim never overturns a much stronger match. The field is accepted at every writing door.\n",
    },
    Page {
        id: "concepts/trajectory-packet",
        markdown: "---\ntype: concept\ntitle: Trajectory Packet\ncreated: 2026-09-08\n---\n\n# Trajectory Packet\n\nWorking-memory capture: one immutable packet under raw trajectories per task, holding the replayable tool-call record. Skill pages generalize across many packets; case pages record one concrete run. Packets are never edited, only the skill and case pages are.\n",
    },
    Page {
        id: "concepts/env-over-config",
        markdown: "---\ntype: concept\ntitle: Environment Wins over Config\ncreated: 2026-09-09\n---\n\n# Environment Wins over Config\n\nServer config resolves env var first, then config toml, then default, per key. The allow_delete knob, for example, is enabled by an env var or a config line, defaulting to false. parse_bool accepts one, true, yes, and on, and junk is silently treated as absent.\n",
    },
    Page {
        id: "concepts/folder-guessing",
        markdown: "---\ntype: concept\ntitle: Folder Guessing\ncreated: 2026-09-11\n---\n\n# Folder Guessing\n\nNever infer the folder from the page type. The sources folder hosts both source and retro pages, so a guessed retros path does not exist and breaks the link. Links must use the exact id a tool returned. The server retargets a dangling link whose basename matches one page, but ambiguity stays broken.\n",
    },
    Page {
        id: "concepts/content-hash-staleness",
        markdown: "---\ntype: concept\ntitle: Content Hash Staleness\ncreated: 2026-09-10\n---\n\n# Content Hash Staleness\n\nA stable fnv style hash of the joined chunk text persists next to the vectors. Reindex skips pages whose hash is unchanged, and a model change invalidates every page. The hash is hand rolled because the standard library hasher is not stable across rust releases.\n",
    },
    Page {
        id: "sources/pdf-text-extraction",
        markdown: "---\ntype: source\ntitle: PDF Text Extraction\nformat: article\ncreated: 2026-09-11\n---\n\n# PDF Text Extraction\n\nCapture converts pdfs with the pdf extract crate. Percent pdf magic bytes override a lying content type, a pdf without a text layer is refused by name because there is no ocr, and lines are tidied. The same converter serves url fetch and local file paths.\n",
    },
    Page {
        id: "sources/luhmann-zettelkasten",
        markdown: "---\ntype: source\ntitle: The Zettelkasten Method of Niklas Luhmann\nformat: paper\ncreated: 2026-09-10\n---\n\n# The Zettelkasten Method of Niklas Luhmann\n\nThe card box note method: atomic permanent notes with fixed addresses, structure notes that branch, and meaningful links between cards. It organizes thinking in plain files long before software existed.\n",
    },
    Page {
        id: "sources/wiki-okf-incompatibility",
        markdown: "---\ntype: source\ntitle: wiki OKF Incompatibility\nformat: note\ncreated: 2026-09-10\n---\n\n# wiki OKF Incompatibility\n\nOld vaults lack the okf marker and guessed the analysis folder name wrong. A one-time migration command repairs them, refuses when a clash would lose data, and is idempotent. The automatic boot migration was removed again, leaving the command line path only.\n",
    },
    Page {
        id: "retros/git-auto-commit-idle-bug",
        markdown: "---\ntype: retro\ntitle: The Git Auto-Commit Idle Bug\nrelevance: high\ncreated: 2026-09-10\n---\n\n# The Git Auto-Commit Idle Bug\n\nThe tick rewrote the meta git json every cycle, and the idle clock counted that bookkeeping as vault activity. A vault idles five minutes of downtime before an auto commit, but the clock never reached it, so exactly one auto commit happened. The status line saying in sync is not proof commits happen.\n",
    },
    Page {
        id: "retros/personal-layer-ranking-bug",
        markdown: "---\ntype: retro\ntitle: The Personal Layer Ranking Bug\ncreated: 2026-09-09\n---\n\n# The Personal Layer Ranking Bug\n\nPersonal layer hits were appended after the sort and could be truncated away even when they were the best match. The fix merged them before the cap so they compete by score. Same sort then cap invariant protects the semantic pass today.\n",
    },
    Page {
        id: "retros/retro-window-clamp",
        markdown: "---\ntype: retro\ntitle: Retro Worker Window Clamp\ncreated: 2026-09-10\n---\n\n# Retro Worker Window Clamp\n\nConsecutive retro runs saw overlapping evidence and wrote near-duplicate pages under different slugs. The fix bounds the evidence window with a since timestamp, passes the transcript as a pre-cut slice, and hands the worker the ids it already recorded so it refuses to restate them.\n",
    },
    Page {
        id: "analyses/embeddings-vs-qmd",
        markdown: "---\ntype: analysis\ntitle: Embeddings vs QMD\ncreated: 2026-09-10\n---\n\n# Embeddings vs QMD\n\nChunk vectors with additive fusion were chosen over a sqlite full text index. FTS handles cjk queries that split into characters; the current tokenizer treats a cjk run as one token, so a non-english query can never be recovered by the lexical engine alone. Fusion also needs a provider, and embedding coverage may be partial.\n",
    },
    Page {
        id: "syntheses/worker-duplicate-generation",
        markdown: "---\ntype: synthesis\ntitle: Worker Duplicate Generation\ncreated: 2026-09-10\n---\n\n# Worker Duplicate Generation\n\nRetro and discovery workers both create pages unattended, and both produced near-duplicate pages because keyword search misses paraphrases. The remedies are semantic search before writing, bounded evidence windows, and refusing recorded ids. Folder guessing also fed the same slugs into the wrong directory once.\n",
    },
];

pub const QUERIES: &[Query] = &[

    // ---- exact_lookup (5: 4 train / 1 heldout) ----
    Query { id: "el-1", text: "wikilink gate modes", category: Category::ExactLookup, split: Split::Train,
        judgments: &[j("concepts/wikilink-gate", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "el-2", text: "embedding store chunk vectors", category: Category::ExactLookup, split: Split::Train,
        judgments: &[j("concepts/embedding-store", 3, Role::Canonical), j("entities/nomic-embed-text-v1-5", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "el-3", text: "relevance claim multiplier", category: Category::ExactLookup, split: Split::Train,
        judgments: &[j("concepts/relevance-claim", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "el-4", text: "okf v0.2 layout", category: Category::ExactLookup, split: Split::Train,
        judgments: &[j("concepts/okf-v0-2", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "el-5", text: "trajectory packet skill distillation", category: Category::ExactLookup, split: Split::Heldout,
        judgments: &[j("concepts/trajectory-packet", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- entity_alias (5: 4 train / 1 heldout) ----
    Query { id: "ea-1", text: "the embedder model with 768 dimensions", category: Category::EntityAlias, split: Split::Train,
        judgments: &[j("entities/nomic-embed-text-v1-5", 3, Role::Canonical), j("concepts/embedding-store", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "ea-2", text: "the remote wiki mcp server in rust", category: Category::EntityAlias, split: Split::Train,
        judgments: &[j("entities/rust-wiki", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ea-3", text: "niklas luhmann card box idea", category: Category::EntityAlias, split: Split::Train,
        judgments: &[j("sources/luhmann-zettelkasten", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ea-4", text: "pdf capture library used by the server", category: Category::EntityAlias, split: Split::Train,
        judgments: &[j("sources/pdf-text-extraction", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ea-5", text: "the open knowledge format the vault follows", category: Category::EntityAlias, split: Split::Heldout,
        judgments: &[j("concepts/okf-v0-2", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- paraphrase (5: 4 train / 1 heldout) ----
    Query { id: "pa-1", text: "turn a document into numbers before storing its meaning", category: Category::Paraphrase, split: Split::Train,
        judgments: &[j("concepts/embedding-store", 3, Role::Canonical), j("concepts/semantic-candidate-admission", 2, Role::Evidence), j("entities/nomic-embed-text-v1-5", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "pa-2", text: "stop writing a new page when the same insight already exists", category: Category::Paraphrase, split: Split::Train,
        judgments: &[j("retros/retro-window-clamp", 3, Role::Canonical), j("syntheses/worker-duplicate-generation", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "pa-3", text: "the box of cards approach to permanent notes", category: Category::Paraphrase, split: Split::Train,
        judgments: &[j("sources/luhmann-zettelkasten", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "pa-4", text: "an env variable beats the config file when both set the knob", category: Category::Paraphrase, split: Split::Train,
        judgments: &[j("concepts/env-over-config", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "pa-5", text: "skip recomputing vectors for pages that have not changed", category: Category::Paraphrase, split: Split::Heldout,
        judgments: &[j("concepts/content-hash-staleness", 3, Role::Canonical), j("concepts/embedding-store", 2, Role::Evidence)], expected_conflicts: &[] },

    // ---- vague_recollection (5: 4 train / 1 heldout) ----
    Query { id: "vr-1", text: "the thing that kept claiming the vault was in sync", category: Category::VagueRecollection, split: Split::Train,
        judgments: &[j("retros/git-auto-commit-idle-bug", 3, Role::Canonical), j("concepts/env-over-config", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "vr-2", text: "those pages that appeared twice under different names", category: Category::VagueRecollection, split: Split::Train,
        judgments: &[j("syntheses/worker-duplicate-generation", 3, Role::Canonical), j("retros/retro-window-clamp", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "vr-3", text: "the odd analysis folder name problem", category: Category::VagueRecollection, split: Split::Train,
        judgments: &[j("sources/wiki-okf-incompatibility", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "vr-4", text: "the square bracket rewrite that mangled my examples", category: Category::VagueRecollection, split: Split::Train,
        judgments: &[j("concepts/wikilink-gate", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "vr-5", text: "personal search results vanishing from the list", category: Category::VagueRecollection, split: Split::Heldout,
        judgments: &[j("retros/personal-layer-ranking-bug", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- conceptual (5: 4 train / 1 heldout) ----
    Query { id: "co-1", text: "why should an agent keep a markdown wiki at all", category: Category::Conceptual, split: Split::Train,
        judgments: &[j("concepts/karpathy-pattern", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "co-2", text: "how does recall decide which pages rank higher", category: Category::Conceptual, split: Split::Train,
        judgments: &[j("concepts/relevance-claim", 2, Role::Evidence), j("concepts/semantic-candidate-admission", 2, Role::Evidence), j("analyses/embeddings-vs-qmd", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "co-3", text: "what is working memory for an agent", category: Category::Conceptual, split: Split::Train,
        judgments: &[j("concepts/trajectory-packet", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "co-4", text: "what is the point of immutability for captured material", category: Category::Conceptual, split: Split::Train,
        judgments: &[j("concepts/trajectory-packet", 2, Role::Evidence), j("concepts/karpathy-pattern", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "co-5", text: "why does a server need a wikilink gate at all", category: Category::Conceptual, split: Split::Heldout,
        judgments: &[j("concepts/wikilink-gate", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- graph_scope (5: 4 train / 1 heldout) ----
    Query { id: "gs-1", text: "what breaks when a page guesses its folder from the type", category: Category::GraphScope, split: Split::Train,
        judgments: &[j("concepts/folder-guessing", 3, Role::Canonical), j("syntheses/worker-duplicate-generation", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "gs-2", text: "which pages link into the retro window clamp idea", category: Category::GraphScope, split: Split::Train,
        judgments: &[j("retros/retro-window-clamp", 3, Role::Canonical), j("syntheses/worker-duplicate-generation", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "gs-3", text: "what follows from the in sync status line being wrong", category: Category::GraphScope, split: Split::Train,
        judgments: &[j("retros/git-auto-commit-idle-bug", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "gs-4", text: "where does the reindex skip logic live with its hash", category: Category::GraphScope, split: Split::Train,
        judgments: &[j("concepts/content-hash-staleness", 3, Role::Canonical), j("concepts/embedding-store", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "gs-5", text: "how is the personal layer merged into the overall recall", category: Category::GraphScope, split: Split::Heldout,
        judgments: &[j("retros/personal-layer-ranking-bug", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- evidence_request (5: 4 train / 1 heldout) ----
    Query { id: "ev-1", text: "what does the luhmann source say about fixed addresses", category: Category::EvidenceRequest, split: Split::Train,
        judgments: &[j("sources/luhmann-zettelkasten", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ev-2", text: "which source documents why ocr was not added", category: Category::EvidenceRequest, split: Split::Train,
        judgments: &[j("sources/pdf-text-extraction", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ev-3", text: "point me at the writeup of the idle clock fix", category: Category::EvidenceRequest, split: Split::Train,
        judgments: &[j("retros/git-auto-commit-idle-bug", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ev-4", text: "who first described the card box note method", category: Category::EvidenceRequest, split: Split::Train,
        judgments: &[j("sources/luhmann-zettelkasten", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ev-5", text: "a source saying embeddings beat a full text index", category: Category::EvidenceRequest, split: Split::Heldout,
        judgments: &[j("analyses/embeddings-vs-qmd", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- temporal (5: 4 train / 1 heldout) ----
    Query { id: "te-1", text: "the bug that appeared after five minutes of inactivity", category: Category::Temporal, split: Split::Train,
        judgments: &[j("retros/git-auto-commit-idle-bug", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "te-2", text: "what changed in a later run of the same worker", category: Category::Temporal, split: Split::Train,
        judgments: &[j("retros/retro-window-clamp", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "te-3", text: "the follow up after the store format was changed", category: Category::Temporal, split: Split::Train,
        judgments: &[j("concepts/content-hash-staleness", 3, Role::Canonical), j("concepts/embedding-store", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "te-4", text: "the week the personal layer fix landed", category: Category::Temporal, split: Split::Train,
        judgments: &[j("retros/personal-layer-ranking-bug", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "te-5", text: "the order of the migration and the marker", category: Category::Temporal, split: Split::Heldout,
        judgments: &[j("sources/wiki-okf-incompatibility", 3, Role::Canonical)], expected_conflicts: &[] },

    // ---- contradiction (5: 4 train / 1 heldout) ----
    Query { id: "ct-1", text: "can a non-english query ever find a page on this engine", category: Category::Contradiction, split: Split::Train,
        judgments: &[j("analyses/embeddings-vs-qmd", 2, Role::Evidence), j("concepts/semantic-candidate-admission", 2, Role::Evidence)], expected_conflicts: &["analyses/embeddings-vs-qmd", "concepts/semantic-candidate-admission"] },
    Query { id: "ct-2", text: "is the in sync status trustworthy or not", category: Category::Contradiction, split: Split::Train,
        judgments: &[j("retros/git-auto-commit-idle-bug", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ct-3", text: "do code samples get rewritten or left alone on write", category: Category::Contradiction, split: Split::Train,
        judgments: &[j("concepts/wikilink-gate", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ct-4", text: "was auto migration shipped or removed", category: Category::Contradiction, split: Split::Train,
        judgments: &[j("sources/wiki-okf-incompatibility", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "ct-5", text: "do worker pages get deduplicated by words alone", category: Category::Contradiction, split: Split::Heldout,
        judgments: &[j("syntheses/worker-duplicate-generation", 2, Role::Evidence), j("retros/retro-window-clamp", 3, Role::Canonical)], expected_conflicts: &["syntheses/worker-duplicate-generation", "retros/retro-window-clamp"] },

    // ---- conclusion (5: 4 train / 1 heldout) ----
    Query { id: "cn-1", text: "is the vault state actually healthy when it says in sync", category: Category::Conclusion, split: Split::Train,
        judgments: &[j("retros/git-auto-commit-idle-bug", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "cn-2", text: "should workers search before they write", category: Category::Conclusion, split: Split::Train,
        judgments: &[j("syntheses/worker-duplicate-generation", 3, Role::Canonical), j("retros/retro-window-clamp", 2, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "cn-3", text: "when is it safe to guess a folder for a link", category: Category::Conclusion, split: Split::Train,
        judgments: &[j("concepts/folder-guessing", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "cn-4", text: "does a relevance claim ever beat a stronger match", category: Category::Conclusion, split: Split::Train,
        judgments: &[j("concepts/relevance-claim", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "cn-5", text: "can a paraphrased page be found without exact words", category: Category::Conclusion, split: Split::Heldout,
        judgments: &[j("concepts/semantic-candidate-admission", 2, Role::Evidence), j("analyses/embeddings-vs-qmd", 1, Role::Evidence)], expected_conflicts: &[] },

    // ---- synthesis (5: 4 train / 1 heldout) ----
    Query { id: "sy-1", text: "how do the workers avoid duplicate pages across runs", category: Category::Synthesis, split: Split::Train,
        judgments: &[j("syntheses/worker-duplicate-generation", 3, Role::Canonical), j("retros/retro-window-clamp", 3, Role::Canonical)], expected_conflicts: &[] },
    Query { id: "sy-2", text: "what makes recall surface the right page for a memory", category: Category::Synthesis, split: Split::Train,
        judgments: &[j("concepts/relevance-claim", 2, Role::Evidence), j("concepts/semantic-candidate-admission", 2, Role::Evidence), j("analyses/embeddings-vs-qmd", 1, Role::Evidence), j("concepts/embedding-store", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "sy-3", text: "how does the server keep captured material trustworthy", category: Category::Synthesis, split: Split::Train,
        judgments: &[j("concepts/trajectory-packet", 2, Role::Evidence), j("concepts/karpathy-pattern", 2, Role::Evidence), j("sources/pdf-text-extraction", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "sy-4", text: "how does the whole ingest to recall pipeline fit together", category: Category::Synthesis, split: Split::Train,
        judgments: &[j("entities/rust-wiki", 2, Role::Evidence), j("concepts/embedding-store", 2, Role::Evidence), j("concepts/semantic-candidate-admission", 2, Role::Evidence), j("concepts/wikilink-gate", 1, Role::Evidence)], expected_conflicts: &[] },
    Query { id: "sy-5", text: "what keeps the wiki healthy across workers and edits", category: Category::Synthesis, split: Split::Heldout,
        judgments: &[j("concepts/folder-guessing", 2, Role::Evidence), j("syntheses/worker-duplicate-generation", 2, Role::Evidence), j("concepts/relevance-claim", 1, Role::Evidence)], expected_conflicts: &[] },

    // ---- negative (5: 4 train / 1 heldout) ----
    Query { id: "ng-1", text: "postgres replication tuning", category: Category::Negative, split: Split::Train, judgments: &[], expected_conflicts: &[] },
    Query { id: "ng-2", text: "kafka consumer group rebalancing", category: Category::Negative, split: Split::Train, judgments: &[], expected_conflicts: &[] },
    Query { id: "ng-3", text: "docker compose healthcheck practices", category: Category::Negative, split: Split::Train, judgments: &[], expected_conflicts: &[] },
    Query { id: "ng-4", text: "borrow checker lifetimes explained", category: Category::Negative, split: Split::Train, judgments: &[], expected_conflicts: &[] },
    // INTENTIONAL GUARANTEED MISS: CJK query, English-only page body. The
    // tokenizer splits on non-alphanumeric and drops short tokens, so the CJK
    // run is one token and the zettelkasten page scores zero by construction.
    // Do NOT reword the page to make this pass — it documents that only
    // CJK-aware tokenization (Phase 2) can recover it. The gate asserts the
    // relevant page stays out of the top results.
    Query { id: "miss-cjk", text: "什么是卡片盒笔记法", category: Category::Negative, split: Split::Heldout,
        judgments: &[j("sources/luhmann-zettelkasten", 3, Role::Canonical)], expected_conflicts: &[] },
];