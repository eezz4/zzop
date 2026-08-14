// 계약 — Contract
// 문장은 여기 한 번만 산다. HTML 은 scripts/site/render.mjs 가 만든다.

export default {
  bands: [
    {
      header: true,
      blocks: [
        ["eyebrow", { ko: "계약", en: "Contract" }],
        [
          "h1",
          {
            ko: "답은 객체 하나다.<br>자리는 실행마다 그대로다.",
            en: "One object comes back.<br>Its slots never move.",
          },
        ],
        [
          "lede",
          {
            ko: `zzop 은 실행마다 <strong>같은 키 집합</strong>을 가진 JSON 객체 하나로 답한다.
      값이 없어도 남는 자리가 있고, 그 능력이 안 돌면 <em>키 자체가 사라지는</em> 자리가 있다 —
      그 둘이 서로 다른 말이기 때문이다.`,
            en: `Every run answers with one JSON object carrying <strong>the same set of keys</strong>.
      Some slots stay even when empty; others <em>vanish entirely</em> when the capability didn't run —
      because those two are not the same statement.`,
          },
        ],
        [
          "muted",
          {
            ko: `이 페이지는 그 자리들이 무엇을 약속하는지만 본다. 오퍼레이션마다의 필드 표는 여기 없다 —
      마지막 밴드에 왜 없는지가 있다.`,
            en: `This page is about what those slots promise. The per-operation field tables are not here —
      the last band says why.`,
          },
        ],
      ],
    },

    {
      tint: true,
      wide: true,
      loose: true,
      blocks: [
        [
          "group",
          { tight: true },
          [
            ["eyebrow", { ko: "보장", en: "Guarantees" }],
            [
              "h2",
              {
                ko: "답 이전에 셋이 먼저 정해져 있다.",
                en: "Three things are settled before the answer is.",
              },
            ],
          ],
        ],
        [
          "lenses",
          [
            {
              h: { ko: "설정이 없으면 답도 없다", en: "No config, no answer" },
              p: {
                ko: `분석은 <strong>거부</strong>된다 — 조용히 작아진 답이 대신 오지 않는다. 그러면서도
          <code>zzop init</code> 이 쓰는 스타터 설정은 아무것도 끄지 않는다: <code>packs.disabled</code> 는 빈 배열,
          <code>rules</code> 는 빈 객체, <code>exclude</code> 는 빈 배열이다.`,
                en: `The run is <strong>refused</strong> — a quietly smaller answer never arrives in its place. And
          the starter config <code>zzop init</code> writes switches nothing off: <code>packs.disabled</code> is an
          empty array, <code>rules</code> an empty object, <code>exclude</code> an empty array.`,
              },
            },
            {
              h: { ko: "좁아진 스코프는 자기 신고한다", en: "A narrowed scope says so" },
              p: {
                ko: `못 본 것 · 못 한 것 · 잘린 것이 전부 <strong>이름 있는 자리</strong>에 적힌다. 그리고 세는 수는
          필터를 걸어도 줄지 않는다 — 줄어드는 건 보여주는 목록뿐이다.`,
                en: `What it couldn't see, couldn't do, or had to cut all land in a <strong>named field</strong>.
          And the counts never shrink when you filter — only the shown list does.`,
              },
            },
            {
              h: { ko: "실패는 값이다", en: "A failure is a value" },
              p: {
                ko: `파일 하나가 파싱에 실패해도 실행은 계속되고, 그 파일은 <code>degraded</code> 에 이름으로 남는다.
          MCP 툴 실패도 프로토콜 오류가 아니라 <code>isError</code> 를 단 보통의 결과라 서버는 살아 있다.`,
                en: `One file failing to parse does not stop the run — that file is named in <code>degraded</code>.
          An MCP tool failure is an ordinary result flagged <code>isError</code>, not a protocol error, so the
          server stays up.`,
              },
            },
          ],
        ],
        // ⚠ 스키마 공백 G1: 원본은 class="note inner" 인데 scripts/site/render.mjs 의 note 는 inner 옵션이 없다
        //   (muted 에는 있다). 아래 {inner:true} 는 지금 무시된다 — 렌더러가 muted 처럼 받으면 그대로 맞는다.
        [
          "note",
          {
            ko: `CLI 는 그 마지막 항목이 다르다. 실패하면 stderr 로 <code>zzop: &lt;메시지&gt;</code> 한 줄이 나가고 종료 코드는
      <code>1</code> 이다 — <strong>JSON 이 아니다</strong>. stdout 은 성공했을 때만 쓰이니, 파이프라인은 stdout 만 파싱하면 된다.`,
            en: `The CLI differs on that last one: a failure prints one <code>zzop: &lt;message&gt;</code> line to stderr and
      exits <code>1</code> — <strong>not JSON</strong>. stdout is written on success only, so a pipeline can parse
      stdout and nothing else.`,
          },
          { inner: true },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "봉투", en: "The envelope" }],
        [
          "h2",
          {
            ko: "한 트리를 분석하면 이게 돌아온다.",
            en: "Analyze one tree, and this comes back.",
          },
        ],
        [
          "p",
          {
            ko: `<code>zzop analyze</code> 가 찍는 것과 MCP <code>analyze_repo</code> 가 돌려주는 것은
      <strong>같은 셰이퍼를 지난 같은 객체</strong>다. 호스트가 자기 모양으로 다시 빚지 않는다.`,
            en: `What <code>zzop analyze</code> prints and what the MCP <code>analyze_repo</code> tool returns are
      <strong>one object through one shaper</strong>. Neither host reshapes it into a dialect of its own.`,
          },
        ],
        [
          "panel",
          {
            tab: {
              ko: "응답 하나 — 필드 이름은 실제, 값은 예시",
              en: "one reply — field names real, values illustrative",
            },
            lines: [
              `{`,
              `  <span class="hit">"path"</span>: "/repo/api",`,
              `  <span class="hit">"config"</span>: "/repo/api/zzop.config.jsonc",`,
              `  <span class="hit">"fileCount"</span>: 1284,`,
              {
                code: `  <span class="hit">"degraded"</span>: [],                 `,
                comment: {
                  ko: "// 구조 추출이 빠진 파일",
                  en: "// files that lost structural extraction",
                },
              },
              `  <span class="hit">"packsLoaded"</span>: [`,
              `    { "id": "security", "rules": 49, "source": "inline", "filesInScope": 912 }`,
              `  ],`,
              `  <span class="hit">"findings"</span>: {`,
              {
                code: `    "total": 137,                    `,
                comment: { ko: "// 필터와 무관한 전체", en: "// the full count, always" },
              },
              `    "bySeverity": { "critical": 3, "warning": 61, "info": 73 },`,
              `    "byRule": { "security/hardcoded-secret": 2 },`,
              {
                code: `    "shown": [ ],                    `,
                comment: { ko: "// 필터·상한이 걸린 목록", en: "// the filtered, capped list" },
              },
              `    "truncated": { "shown": 50, "totalMatching": 137, "hint": "..." }`,
              `  },`,
              `  <span class="hit">"warnings"</span>: [ ],`,
              `  <span class="hit">"coverage"</span>: {`,
              `    "files": 1284, "parserDispatched": 1102, "symbols": 8431,`,
              `    "resolvedImportEdges": 3126,`,
              `    "ioProvides": 84, "ioConsumesKeyed": 57, "ioConsumesUnresolved": 12,`,
              `    "degraded": 0, <span class="hit">"joinContributionZero"</span>: false`,
              `  },`,
              `  <span class="hit">"configWarnings"</span>: [ ],`,
              `  <span class="hit">"disclosure"</span>: { "classes": 18, "asserted": 6, "partial": 10, "notYetDetected": 2 },`,
              `  <span class="hit">"gitWindow"</span>: { "recentDays": 30, "since": null }`,
              `}`,
              // ⚠ 스키마 공백 G2: 원본은 ko 스팬과 en 스팬 **사이**에 언어 무관 스팬이 하나 끼어 있다
              //   (필드 이름 줄). panel 라인은 [code][ko][en] 순서만 낼 수 있어 그 자리가 없다.
              //   아래 mid 는 지금 무시된다 — 렌더러가 ko 와 en 사이에 <span class="c">mid</span> 를 끼우면 그대로 맞는다.
              //   개행은 스팬 안쪽에 둔다: 숨겨진 쪽이 빈 줄을 남기지 않게.
              {
                comment: {
                  ko: "  위 열한 자리는 언제나 있다. 조건이 맞을 때만 붙는 자리는 따로다 —\n",
                  en: "\n  the eleven above are always present; the conditional ones are on the line before this",
                },
                mid: "  ruleOverridesApplied · architecture · ruleTimings · degradedTruncated",
              },
            ],
          },
        ],
        [
          "p",
          {
            ko: `값이 없을 때 무엇을 하느냐가 이 봉투의 진짜 계약이다. 셋이 서로 다른 뜻을 갖는다.`,
            en: `What happens when there is no value is where this envelope's real contract lives. Three different answers.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "빈 배열", en: "An empty array" },
              v: {
                ko: `<code>warnings</code> · <code>configWarnings</code> · <code>packsLoaded</code> 는 비어도 항상 나온다. 빈 배열이 곧 <strong>“할 말이 없었다”</strong>는 답이다.`,
                en: `<code>warnings</code>, <code>configWarnings</code> and <code>packsLoaded</code> ship even when empty — the empty array <strong>is</strong> the answer, "nothing to report".`,
              },
            },
            {
              k: { ko: "사라진 키", en: "A missing key" },
              v: {
                ko: `<code>ruleOverridesApplied</code> · <code>architecture</code> 는 그 능력이 안 돌면 <strong>키 자체가 없다</strong>. <code>null</code> 을 찍어 “쟀는데 아무것도 없었다”처럼 보이게 하지 않는다.`,
                en: `<code>ruleOverridesApplied</code> and <code>architecture</code> are <strong>absent</strong> when the capability didn't run — never a <code>null</code> that reads as "measured, came out empty".`,
              },
            },
            {
              k: "<code>null</code>",
              v: {
                ko: `<code>gitWindow: null</code> 은 git 신호가 안 돌았다는 뜻이고, <code>architecture.pain: null</code> 은 <strong>잴 모집단이 없었다</strong>는 뜻이다 — <code>0</code> 이 아니다.`,
                en: `<code>gitWindow: null</code> means git signals never ran; <code>architecture.pain: null</code> means <strong>no metric had a population</strong> — which is not <code>0</code>.`,
              },
            },
          ],
        ],
      ],
    },

    {
      tint: true,
      wide: true,
      loose: true,
      blocks: [
        [
          "group",
          { tight: true },
          [
            ["eyebrow", { ko: "자기 신고", en: "Self-report" }],
            ["h2", { ko: "줄어든 자리마다 이름이 붙는다.", en: "Every narrowing has a name." }],
            [
              "lede",
              {
                ko: `정적 분석에서 제일 위험한 건 틀린 답이 아니라 <em>조용히 작아진 답</em>이다. 그래서 스코프를 줄이는 것마다 그 사실이 나가는 자리가 따로 있다.`,
                en: `The hazard is not a wrong answer but <em>an answer that quietly got smaller</em>. So every thing that narrows the scope has a field of its own to say so.`,
              },
            ],
          ],
        ],
        [
          "vs",
          [
            {
              k: "<code>findings.total</code>",
              v: {
                ko: `언제나 전체다. <code>bySeverity</code> · <code>byRule</code> 도 같다. <code>--severity</code> · <code>--rule</code> · <code>--limit</code> 이 줄이는 것은 <code>shown</code> 뿐이라, 인용한 수가 필터 때문에 작아질 일이 없다.`,
                en: `Always the whole set, and so are <code>bySeverity</code> and <code>byRule</code>. <code>--severity</code>, <code>--rule</code> and <code>--limit</code> move <code>shown</code> and nothing else, so a number you quote can't shrink because of your filter.`,
              },
            },
            {
              k: "<code>findings.truncated</code>",
              v: {
                ko: `잘렸을 때만 나오고, <code>{shown, totalMatching, hint}</code> 셋을 같이 준다. <code>hint</code> 에는 <strong>이 목록에 실제로 먹는 방법</strong>만 적힌다 — 고정 상한인 목록에는 “limit 을 올려라”라고 쓰지 않는다.`,
                en: `Present only when the cut bit, carrying <code>{shown, totalMatching, hint}</code>. The <code>hint</code> names <strong>a remedy that actually works on that list</strong> — a fixed-cap list is never told to "raise the limit".`,
              },
            },
            {
              k: "<code>packsLoaded[].filesInScope</code>",
              v: {
                ko: `<code>0</code> 이면 그 팩은 로드됐지만 이번 트리에 대상 파일이 하나도 없었다. 발견 0 이 <strong>“깨끗하다”가 아니라 “범위 밖”</strong>이라는 뜻이다.`,
                en: `A <code>0</code> means the pack loaded but no analyzed file was in any of its rules' scope: zero findings is <strong>"out of scope", not "clean"</strong>.`,
              },
            },
            {
              k: "<code>ruleOverridesApplied</code>",
              v: {
                ko: `당신이 끈 것이 <strong>실제로 꺼졌다는 확인</strong> — <code>{disabled, severityRemapped, only}</code>. 오타 난 룰 id 는 여기 안 들어오고 <code>configWarnings</code> 로 간다.`,
                en: `Positive confirmation that what you switched off <strong>actually took effect</strong> — <code>{disabled, severityRemapped, only}</code>. A mistyped rule id never lands here; it lands in <code>configWarnings</code>.`,
              },
            },
            {
              k: "<code>coverage.joinContributionZero</code>",
              v: {
                ko: `파일은 읽었는데 이을 수 있는 io 가 0 이었다는 <strong>단언</strong>. 이 트리는 조인에서 자기가 안 보인다고 스스로 말한다.`,
                en: `An <strong>assertion</strong> that the tree read files yet extracted no joinable io — it says outright that it is invisible to the cross-layer join.`,
              },
            },
            {
              k: "<code>warnings</code>",
              v: {
                ko: `못 한 일뿐 아니라 <strong>효과가 없었던 선언</strong>까지 적는다 — 예: 마운트 하나가 http provide 를 단 하나도 옮기지 못했을 때, 그 마운트 이름과 함께.`,
                en: `Not only what it couldn't do but <strong>declarations that changed nothing</strong> — a topology mount that moved zero http provides is reported by name.`,
              },
            },
          ],
          { wide: true },
        ],
        // ⚠ 스키마 공백 G1 (위와 같은 자리): class="note inner".
        [
          "note",
          {
            ko: `<code>disclosure</code> 는 이번 실행이 아니라 <strong>zzop 자신</strong>에 대한 자리다 — 아직 못 잡는 침묵의 종류를 센다.
      지금 <strong>18종</strong>이고 그중 <strong>12종</strong>은 부분 탐지이거나 아예 탐지 못 한다. 매 실행 같은 글이라
      전문은 응답에서 빼고 <code>zzop contract disclosure-classes</code> 로 옮겼다 — 숫자는 남는다.`,
            en: `<code>disclosure</code> is not about this run but about <strong>zzop itself</strong>: it counts the classes of
      silence zzop does not yet catch. <strong>18</strong> today, <strong>12</strong> of them only partially detected
      or not at all. The text is identical every run, so it ships once via
      <code>zzop contract disclosure-classes</code> — the counts stay in the reply.`,
          },
          { inner: true },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "여러 저장소", en: "Several repositories" }],
        [
          "h2",
          {
            ko: "조인은 옵션이 아니라 다른 답이다.",
            en: "The join isn't a mode. It's a different answer.",
          },
        ],
        [
          "p",
          {
            ko: `<code>zzop cross</code> 는 트리를 <strong>둘 이상</strong> 받는다 — 하나로는 부를 수 없다.
      돌아오는 것은 단일 트리 응답을 여러 개 담은 배열이 아니라 <em>다른 모양의 객체</em>다:
      트리별 요약 <code>sources[]</code> 와, 조인 자체의 결과가 나란히 있다.`,
            en: `<code>zzop cross</code> takes <strong>two or more</strong> trees — one is not a valid call. What comes back
      is not an array of single-tree replies but <em>a different object</em>: the per-tree summaries in
      <code>sources[]</code>, and beside them the join's own result.`,
          },
        ],
        [
          "panel",
          {
            tab: {
              ko: "조인 응답 — 필드 이름은 실제, 값은 예시",
              en: "the join reply — field names real, values illustrative",
            },
            lines: [
              `{`,
              {
                code: `  "config": null,                    `,
                comment: {
                  ko: "// 루트를 직접 넘긴 모드",
                  en: "// paths mode: no single config governs it",
                },
              },
              `  <span class="hit">"sources"</span>: [`,
              `    { "sourceId": "web", "path": "/repo/web", "fileCount": 812,  "findingCount": 44,  "coverage": { } },`,
              `    { "sourceId": "api", "path": "/repo/api", "fileCount": 1284, "findingCount": 137, "coverage": { } }`,
              `  ],`,
              `  <span class="hit">"buckets"</span>: {`,
              {
                code: `    "edges": 61,                     `,
                comment: { ko: "// 이어진 쌍", en: "// matched consume-&gt;provide pairs" },
              },
              {
                code: `    "unconsumedProvides": 9,         `,
                comment: { ko: "// 부르는 데가 없는 라우트", en: "// routes nobody calls" },
              },
              {
                code: `    "unprovidedConsumes": 23,        `,
                comment: { ko: "// 받는 데가 없는 호출", en: "// calls nothing serves" },
              },
              `    "unresolvedConsumes": 7, "externalConsumes": 4, "ambiguousConsumes": 0`,
              `  },`,
              {
                code: `  "bucketMeaning": "...",            `,
                comment: {
                  ko: "// 위 여섯 수의 산술을 응답이 직접 적는다",
                  en: "// the arithmetic, on the wire",
                },
              },
              `  <span class="hit">"distinctBucketKeys"</span>: { "unprovidedConsumes": ["PUT /api/user"] },`,
              `  "distinctBucketKeyFirstSites": { },`,
              `  "edges": [ ],`,
              `  "crossLayerFindings": { "total": 5 },`,
              `  "configWarnings": [ ], "warnings": [ ], "disclosure": { }`,
              `}`,
            ],
          },
        ],
        [
          "p",
          {
            ko: `<code>buckets</code> 는 <strong>행</strong>을 세고 <code>distinctBucketKeys</code> 는 그 행들이 접히는
      <strong>키</strong>를 나열한다. 그래서 둘의 길이가 다른 게 정상이고 — 같은 라우트를 세 군데서 부르면 행 셋, 키 하나다 —
      그 관계를 응답 자신이 <code>bucketMeaning</code> 에 적어 둔다. 읽는 쪽이 문서를 찾아가 확인할 일이 없다.`,
            en: `<code>buckets</code> counts <strong>rows</strong>; <code>distinctBucketKeys</code> lists the <strong>keys</strong>
      those rows collapse into. The two legitimately differ — one route called from three places is three rows and one
      key — and the reply states that relationship itself, in <code>bucketMeaning</code>, so no reader has to go find a
      document to check it.`,
          },
        ],
        [
          "note",
          {
            ko: `그리고 조인은 <strong>넘겨주지 않은 것도 말한다</strong>. 분석한 루트들이 한 부모 디렉터리 밑에 모여 있으면,
      그 부모의 <em>분석되지 않은 형제 디렉터리</em> 이름이 <code>configWarnings</code> 에 나열된다 —
      조인이 “당신이 마침 넘긴 트리들”로 조용히 좁아지지 않는다.`,
            en: `The join also reports <strong>what it was not given</strong>: when every analyzed root sits under one common
      parent, that parent's <em>unanalyzed sibling directories</em> are named in <code>configWarnings</code> — so the
      join never quietly narrows to "the trees you happened to pass".`,
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "나머지", en: "The rest" }],
        [
          "h2",
          {
            ko: "표는 페이지가 아니라 테스트가 지킨다.",
            en: "A field table is kept honest by a test, not by a page.",
          },
        ],
        [
          "p",
          {
            ko: `오퍼레이션 하나하나의 요청 필드·출력 필드 전체 표는 원문 레퍼런스에 있다. 여기 옮겨 적지 않는 이유는 분량이 아니다 —
      요청 필드 표는 <strong>디시리얼라이저가 실제로 받는 필드 목록</strong>과, 출력 필드 표는 아래 레지스트리와
      Rust 메타테스트가 대조한다. 사본을 여기 만들면 그 사본만 테스트 밖에 있게 된다.`,
            en: `The full request- and output-field tables for every operation live in the source reference. Not copying them here
      is not about length: Rust meta-tests check the request table against <strong>the field list the deserializer
      actually accepts</strong>, and the output table against the registry below. A copy made here would be the one copy
      standing outside that check.`,
          },
        ],
        [
          "p",
          {
            ko: `응답이 무엇을 <strong>떨어뜨렸는지</strong>도 등록돼 있다. 엔진이 계산한 최상위 필드는 전부 레지스트리에 한 행씩 갖고,
      전달 표면이 그것을 그대로 싣는지 · 조건부로 싣는지 · 아예 안 싣는지를 적는다. 지금 <strong>28행</strong>이고
      그중 <strong>9행</strong>이 “안 실음”이다 — 그리고 안 싣는 행은 <em>그 값을 어디서 얻는지</em>를 같이 적어야 통과한다.`,
            en: `What the reply <strong>drops</strong> is registered too. Every top-level field the engine computes gets exactly one
      row saying whether the delivery surface carries it, carries it conditionally, or omits it — <strong>28</strong> rows
      today, <strong>9</strong> of them omissions, and an omission row does not pass unless it also names <em>where that
      value can be had instead</em>.`,
          },
        ],
        [
          "note",
          {
            ko: `오퍼레이션 열두 개 각각의 필드 표는 <a href="https://eezz4.github.io/zzop/reference.html" target="_blank" rel="noreferrer">원문 레퍼런스</a>에 있다.
      이 페이지가 링크만 거는 이유가 그것이다 — 표에는 주인이 하나여야 하고, 그 주인은 <em>테스트가 읽는 쪽</em>이다.`,
            en: `Per-operation field tables for all twelve operations live in the
      <a href="https://eezz4.github.io/zzop/reference.html" target="_blank" rel="noreferrer">source reference</a>.
      That is why this page only links: a table needs exactly one owner, and the owner is <em>the copy a test reads</em>.`,
          },
        ],
      ],
    },
  ],
};
