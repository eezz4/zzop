// 구조 — How it works
// 문장은 여기 한 번만 산다. HTML 은 scripts/site/render.mjs 가 만든다.

export default {
  bands: [
    {
      header: true,
      blocks: [
        ["eyebrow", { ko: "구조", en: "How it works" }],
        [
          "h1",
          {
            ko: "한 파일씩 읽고,<br>한 번에 합친다.",
            en: "Read each file once.<br>Join them all at once.",
          },
        ],
        [
          "lede",
          {
            ko: `zzop 은 저장소를 훑어 파일마다 <strong>같은 모양의 사실</strong>을 뽑는다.
      언어가 무엇이든 결과는 하나의 중립 표현이다.
      그 표현들을 합쳐 그래프를 만들고, 판정은 전부 그 그래프 위에서 한다.`,
            en: `zzop walks a repository and pulls <strong>the same shape of fact</strong> out of every file.
      Whatever the language, the result is one neutral representation.
      Those are merged into a graph, and every judgment is made on that graph.`,
          },
        ],
        [
          "muted",
          {
            ko: `이 페이지는 개념만 다룬다. 필드 하나하나의 모양은 <a href="#p-contract">계약</a>이 주인이다.`,
            en: `This page is conceptual only — the field-by-field shape lives in the <a href="#p-contract">Contract</a>.`,
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "파이프라인", en: "Pipeline" }],
        [
          "h2",
          {
            ko: "네 단계, 그리고 매번 같은 답.",
            en: "Four stages, and the same answer every time.",
          },
        ],
        [
          "p",
          {
            ko: `한 파일에 필요한 일은 한 번에 끝난다. 파싱하고, 중립 표현으로 접고, 룰을 돌리는 것이 한 패스다.
      파일끼리는 병렬로 돈다. 파서가 만든 원본 AST 는 이 단계 밖으로 <strong>나가지 않는다</strong>.`,
            en: `Everything one file needs happens in a single pass: parse it, fold it into the neutral form, run the rules.
      Files go in parallel. The parser's raw AST <strong>never leaves</strong> that step.`,
          },
        ],
        [
          "run",
          [
            {
              cmd: "Walk",
              what: {
                ko: "파일을 모은다 · gitignore 를 따른다",
                en: "collect the files · gitignore-aware",
              },
            },
            {
              cmd: "Parse → IR → rules",
              what: { ko: "파일마다 한 패스", en: "one fused pass per file" },
            },
            {
              cmd: "Assemble",
              what: { ko: "트리 전체를 한 그래프로", en: "one graph for the whole tree" },
            },
            {
              cmd: "Envelope",
              what: { ko: "한 장의 JSON", en: "one JSON out" },
            },
          ],
        ],
        [
          "p",
          {
            ko: `파일들이 처음 만나는 곳은 세 번째 단계다. 순환 의존, 죽은 코드, 구조 점수가 여기서 나온다.
      여러 트리를 함께 분석하면 교차 계층 조인도 여기서 돈다.`,
            en: `Stage three is where the files first meet each other: circular dependencies, dead code and structural scores
      come out here — and, when several trees are analyzed together, so does the cross-layer join.`,
          },
        ],
        [
          "p",
          {
            ko: `언어마다 다른 문법은 두 번째 단계에서 사라진다. 어떤 파일이든 <code>CommonIr</code> 의 같은 네 칸으로 접힌다.
      뒤에 오는 분석은 원본 문법이 아니라 이 네 칸만 본다.`,
            en: `Language-specific syntax ends at stage two: every file folds into the same four slots of <code>CommonIr</code>,
      and everything downstream reads those four slots rather than any original syntax.`,
          },
        ],
        [
          "vs",
          [
            {
              k: "<code>dep</code>",
              v: {
                ko: "임포트 그래프. 어떤 파일이 어떤 파일을 부르나.",
                en: "The import graph — which file pulls in which.",
              },
            },
            {
              k: "<code>symbols</code>",
              v: {
                ko: "함수 · 클래스 · 상수 · 타입 · 인터페이스 선언.",
                en: "Function, class, const, type and interface declarations.",
              },
            },
            {
              k: "<code>loc</code>",
              v: {
                ko: "파일당 물리적 줄 수.",
                en: "Physical line count, per file.",
              },
            },
            {
              k: "<code>io</code>",
              v: {
                ko: "이 파일이 <strong>제공하는 것</strong>과 <strong>소비하는 것</strong> — HTTP 라우트, DB 테이블, tRPC 프로시저. 저장소를 잇는 재료가 여기다.",
                en: "What this file <strong>provides</strong> and <strong>consumes</strong> — HTTP routes, DB tables, tRPC procedures. This is the material the cross-repo join runs on.",
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: "걷는 순서부터 고정돼 있다. 그래서 같은 입력이면 출력이 바이트까지 같다.",
            en: "The order is fixed from the walk onward — which is why the same input gives byte-identical output.",
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "언어", en: "Languages" }],
        [
          "h2",
          {
            ko: "여덟은 직접 읽고, 나머지는 받아 적는다.",
            en: "Eight it parses itself. The rest are handed in.",
          },
        ],
        [
          "p",
          {
            ko: `언어 지원을 <em>되냐 안 되냐</em>로 말하지 않는다.
      각 언어를 <em>무엇이 읽는지</em>를 등급으로 밝힌다 — 그리고 거기서 그 파서가 뒤에 설 수 있는 정밀도가 나온다.`,
            en: `Support is not a <em>yes or no</em> flag. Each language is disclosed as a tier naming <em>what reads it</em>,
      and the precision that parser can stand behind follows from that.`,
          },
        ],
        [
          "vs",
          [
            {
              k: "TypeScript · Python · Rust",
              v: {
                ko: "<code>Full AST</code> — 각 언어의 정식 파서를 라이브러리로 링크했다(swc · ruff · syn 2). 심볼, 임포트, 라우트, 외부 호출, ORM 테이블까지.",
                en: "<code>Full AST</code> — each language's own parser, linked as a library (swc · ruff · syn 2): symbols, imports, routes, outbound calls, ORM tables.",
              },
            },
            {
              k: "Go · Java · C#",
              v: {
                ko: "<code>Full CST</code> — tree-sitter 문법으로 읽는다. gin · Spring MVC · ASP.NET Core 라우트와 GORM · JPA · EF Core 테이블이 같은 채널로 들어온다.",
                en: "<code>Full CST</code> — read through tree-sitter grammars: gin, Spring MVC and ASP.NET Core routes, GORM, JPA and EF Core tables, all into the same channels.",
              },
            },
            {
              k: "Prisma · SQL",
              v: {
                ko: "<code>Lexical</code> — 스키마만 읽는다. Prisma 모델과 <code>CREATE TABLE</code> 이 테이블을 제공하고, 코드 쪽 쿼리가 그것을 소비한다.",
                en: "<code>Lexical</code> — schemas only. Prisma models and <code>CREATE TABLE</code> statements provide the tables that queries elsewhere consume.",
              },
            },
            {
              k: { ko: "그 밖 전부", en: "Everything else" },
              v: {
                ko: "<code>External adapter</code> — 정규화 AST 봉투로 같은 모양을 직접 넣는다. 한 트리를 통째로 대신하거나(Mode A), 네이티브 분석 위에 얹는다(Mode B).",
                en: "<code>External adapter</code> — hand the same shape in through the Normalized AST envelope: stand in for a whole tree (Mode A), or overlay facts onto a natively parsed one (Mode B).",
              },
            },
          ],
          { wide: true },
        ],
        [
          "note",
          {
            ko: `<strong><code>Full AST</code> 와 <code>Full CST</code> 를 가르는 것은 누가 그 파일을 읽느냐 — 그리고 그래서 실패의 결이 다르다.</strong>
      <code>Full AST</code> 는 그 언어 자신의 파서를 링크한 것이라 그 언어의 도구체인이 보는 트리를 그대로 보고,
      파싱이 실패하면 <strong>파일 단위로</strong> 어휘 폴백으로 강등된다.
      <code>Full CST</code> 는 tree-sitter 문법 — 버전이 핀된 독립 재구현이라 오류에 관대하다:
      한 멤버가 깨져도 <strong>파일의 나머지는 계속 추출된다</strong>.
      그 대가로 아주 새로운 문법은 일반 CST 로 파싱은 되지만 <strong>전용 추출이 아직 없을 수 있다</strong>(Java 21 의 sealed-permits 와 패턴 스위치가 그 자리다).
      <strong>등급은 능력 순위가 아니다</strong> — 어떤 채널이 실제로 나오는지는 등급이 아니라 언어마다 다르고,
      그 목록의 정본은 레포의 <code>docs/ARCHITECTURE.md</code> 언어별 표다.`,
            en: `<strong>What separates <code>Full AST</code> from <code>Full CST</code> is who reads the file — and therefore the grain of failure.</strong>
      <code>Full AST</code> links the language's own parser, so it sees the tree that language's own toolchain sees,
      and a file that fails to parse degrades to the lexical fallback <strong>whole</strong>.
      <code>Full CST</code> reads through a tree-sitter grammar — an independent reimplementation, pinned to a version,
      and error-tolerant: one broken member <strong>does not blank the rest of the file</strong>.
      The price is that very new syntax may parse as ordinary CST while <strong>carrying no dedicated extraction yet</strong>
      (Java 21's sealed-permits and pattern switches sit exactly there).
      <strong>The tier is not a capability ranking</strong> — which channels a language actually produces varies by language,
      not by tier, and the repo's per-language table in <code>docs/ARCHITECTURE.md</code> owns that list.`,
          },
        ],
        [
          "muted",
          {
            ko: "파서는 전부 Rust 안에 있다. Python 을 읽는 데 Python 런타임이 필요 없다.",
            en: "Every parser lives inside the Rust binary — reading Python needs no Python runtime.",
          },
        ],
        [
          "p",
          {
            ko: `네이티브 파서가 없는 파일도 버리지 않는다. 줄 수는 세고, 텍스트를 훑는 룰은 그대로 돈다.
      빠지는 것은 심볼 · 임포트 · IO 이고, <strong>빠졌다는 사실이 경고로 적힌다</strong>.`,
            en: `A file with no native parser isn't dropped: it still gets a line count and still runs every text-scanning rule.
      What's missing is symbols, imports and IO — and <strong>the fact that it is missing gets written into the warnings</strong>.`,
          },
        ],
        [
          "note",
          {
            ko: `어떤 언어를 네이티브로 읽을지는 <em>탐지가 되느냐</em>가 아니라 <em>얼마나 흔한 환경이냐</em>로 정한다.
      흔하지 않은 것은 어댑터가 넣는다 — 그래서 목록이 짧은 것이 한계가 아니다.`,
            en: `What gets a native parser is decided by how common an environment is, not by what happens to be detectable.
      Anything niche arrives through an adapter instead — so a short list is not a ceiling.`,
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "교차 계층", en: "Cross-layer" }],
        [
          "h2",
          {
            ko: "두 저장소를 잇는 것은 문자열 하나다.",
            en: "What joins two repositories is a single string.",
          },
        ],
        [
          "p",
          {
            ko: `파서는 파일마다 두 가지를 적어 둔다 — 여기가 <strong>제공하는 것</strong>과 <strong>소비하는 것</strong>.
      여러 트리를 함께 분석하면 이 둘을 정규화된 키로 맞춘다.
      AST 를 맞추는 것이 아니라, 키가 정확히 같은지만 본다.`,
            en: `Every parser records two things per file: what this file <strong>provides</strong> and what it <strong>consumes</strong>.
      Analyze several trees together and the two sides are matched on a normalized key —
      not by matching ASTs, but by asking whether the keys are exactly equal.`,
          },
        ],
        [
          "panel",
          {
            tab: { ko: "조인 키", en: "The join key" },
            lines: [
              `consume  @ fe-vite     fetch("/users/:id")       →  http  GET /users/:id`,
              `provide  @ be-express  router.get('/users/:id')  →  http  GET /users/:id`,
              `                                                          <span class="hit">^^^^^^^^^^^^^^</span>`,
              {
                code: "",
                comment: {
                  ko: "   키가 정확히 같으면 엣지 하나",
                  en: "   exactly equal, so one edge",
                },
              },
            ],
          },
        ],
        [
          "p",
          {
            ko: `그래서 조잡한 외부 어댑터도 낄 수 있다. 키만 제대로 만들면 네이티브 파서와 동등한 참가자다.
      두 저장소가 디스크에서 아무것도 공유하지 않아도 상관없다.`,
            en: `That is why even a crude external adapter can take part: get the key normalization right and it is a first-class
      participant. The two repositories need share nothing on disk.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "이어졌다", en: "Matched" },
              v: {
                ko: "양쪽이 맞으면 엣지 하나. 다만 <code>/login</code> 처럼 아무 서비스나 가질 법한 경로로 맞은 엣지는 <strong>신뢰도 낮음</strong>으로 표시된다.",
                en: "Both sides agree, so an edge is emitted — but an edge matched on a path any service could own (<code>/login</code> and the like) is flagged <strong>low confidence</strong>.",
              },
            },
            {
              k: { ko: "한쪽만 있다", en: "One side only" },
              v: {
                ko: "부르는 데가 없는 라우트와, 받는 데가 없는 호출로 나뉜다. 저장소 사이의 드리프트가 여기서 보인다.",
                en: "Split into routes nobody calls and calls nothing serves. This is where drift between repositories surfaces.",
              },
            },
            {
              k: { ko: "못 봤다", en: "Couldn't tell" },
              v: {
                ko: "키를 정할 수 없던 호출, 후보 트리가 둘 이상인 호출, 외부 호스트로 나가는 호출은 각각 따로 담긴다. <strong>추측해서 잇지 않는다.</strong>",
                en: "Calls whose key couldn't be resolved, calls with two or more candidate trees, and calls to an external host each get their own bucket. <strong>Nothing is joined on a guess.</strong>",
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `게이트웨이가 붙이는 접두사, 그리고 어떤 트리가 어떤 호스트를 소유하는지는 어느 저장소의 코드에도 없다.
      그건 설정(<code>mounts</code> · <code>hosts</code>)으로 선언한다. 선언이 아무 호출도 옮기지 못하면 그 사실도 경고로 나온다.`,
            en: `A gateway's mount prefix, and which hosts a tree owns, exist in neither repository's source — you declare them in
      config (<code>mounts</code> · <code>hosts</code>). If a declaration moves nothing, that too is reported as a warning.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "공시", en: "Disclosure" }],
        [
          "h2",
          { ko: "못 본 자리에는 이름이 붙는다.", en: "A blind spot gets a name." },
        ],
        [
          "p",
          {
            ko: `분석이 완전할 수 없다는 것은 전제다. 그래서 zzop 은 <strong>못 한 일을 결과 안에 적는다</strong>.
      침묵과 무결과를 구별할 수 있어야 초록이 뜻을 가진다.`,
            en: `Incompleteness is the premise, so zzop <strong>writes what it could not do into the result itself</strong>.
      Green only means something when silence can be told apart from "found nothing".`,
          },
        ],
        [
          "vs",
          [
            {
              k: "<code>degraded</code>",
              v: {
                ko: "너무 크거나(기본 상한 1.5MB) 파싱에 실패한 파일. 줄 수와 텍스트 룰은 그대로 돌고 구조 추출만 빠진다.",
                en: "Files too large (1.5MB by default) or that failed to parse. Line counts and text-scanning rules still run; only structural extraction is skipped.",
              },
            },
            {
              k: "<code>warnings</code>",
              v: {
                ko: "이번 실행이 못 한 일을 문장으로 적는다. 파서 없는 확장자, 상한을 넘긴 파일, 효과 없는 설정 선언까지. 파일마다가 아니라 묶어서 한 줄로.",
                en: "What this run could not do, in prose: extensions with no parser, files over the cap, config declarations that changed nothing — aggregated into one entry, never one per file.",
              },
            },
            {
              k: "<code>coverage</code>",
              v: {
                ko: "채널별로 얼마나 채웠는지 센다. 파일은 읽었는데 IO 가 0 이면, 그 트리는 조인에서 자기가 보이지 않는다고 스스로 말한다.",
                en: "A count of how much of each channel actually got filled. If a tree read files but extracted zero IO, it says outright that it is invisible to the join.",
              },
            },
            {
              k: "<code>disclosure</code>",
              v: {
                ko: "아직 잡아내지 못하는 실패의 <em>종류</em>를 이름으로 나열한다. 목록 자체가 출력에 들어 있다.",
                en: "Names the <em>classes</em> of silent failure zzop does not yet detect. The list ships inside the output.",
              },
            },
          ],
        ],
        [
          "p",
          {
            ko: `번들 산출물처럼 한 줄이 지나치게 긴 파일은 또 다른 경우다. 텍스트 룰은 전부 건너뛰지만 구조 추출은 정상으로 돈다.
      거대한 한 줄에는 룰이 기댈 문맥이 없기 때문이다.`,
            en: `A file built of enormous single lines — bundler output and its kin — is a separate case: every text-scanning rule
      is skipped while structural extraction proceeds as normal, because a giant line gives a rule no context to scope to.`,
          },
        ],
        [
          "note",
          {
            ko: `여기까지가 개념이다. 실제 필드 이름과 모양은 <a href="#p-contract">계약</a>, 룰 하나하나는 <a href="#p-rules">룰</a>,
      이 저장소를 실제로 분석한 결과는 <a href="#p-graph">그래프</a>가 주인이다.`,
            en: `That is the concept. Field names and shapes live in the <a href="#p-contract">Contract</a>, individual rules in
      <a href="#p-rules">Rules</a>, and this repository actually analyzed in <a href="#p-graph">Graph</a>.`,
          },
        ],
      ],
    },
  ],
};
