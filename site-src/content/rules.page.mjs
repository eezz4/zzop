// 룰 카탈로그 페이지(독립 `site/rules.html`)의 문장들.
//
// ⚠ `rules.mjs` 와 다르다. 그쪽은 index 의 룰 탭(`#p-rules`)이고 이쪽은 전체 카탈로그 페이지다.
//
// **표 안의 룰 행은 여기 없다.** 그 행들(팩 11개 · 네이티브 분석 · 매처 표)은
// `scripts/gen-site-rules.mjs` 가 `docs/rules/catalog.md` 에서 생성해 템플릿에 채운다. 그래서 이 파일이
//드는 것은 **손으로 쓴 크롬**뿐이다 — 제목과 그 표들을 소개하는 문단. 룰 id·매처 이름·설명은
// 사용자가 config 에 타이핑하는 이름이고 룰의 `message` 로도 그대로 출하되므로 영어로 남는다.
// 같은 이유로 `<code>` 하나뿐인 팩 제목(`<code>db</code>` 등)은 두 판이 같다.

export default {
  head: {
    // `en` 은 이미 발행된 문장 그대로. `head` 는 슬롯이 아니라 손으로 쓰는 자리라 영어를 "더 낫게"
    // 다시 쓰기 쉽고, 앞선 두 페이지에서 실제로 그랬다가 되돌렸다.
    title: {
      ko: `zzop — 룰`,
      en: `zzop — Rules`,
    },
    description: {
      ko: `zzop 룰 카탈로그 — DSL 팩, 매처 레퍼런스, 네이티브 분석, 그리고 커스텀 파서로 넓히기.`,
      en: `zzop rule catalog — DSL packs, matcher reference, native analyses, and extending with custom parsers.`,
    },
  },

  slots: {
    h11: {
      ko: `룰 카탈로그`,
      en: `Rule catalog`,
    },

    p2: {
      ko: `이 페이지는 <code>scripts/gen-site-rules.mjs</code> 가 <code>docs/rules/catalog.md</code> 에서 옮겨 적은 것이다 — 그 파일은 동시에, 바이트 그대로, 에이전트가 읽는 <code>rule-catalog</code> 계약 문서다 — 그리고 메타 테스트(<code>crates/engine/tests/rule_contracts/</code>)가 아래 나열된 모든 id 가 엔진이 런타임에 실제로 싣는 것과 일치하는지 기계로 검사한다. 그래서 카탈로그가 코드와 조용히 어긋날 수 없다. 아래 DSL 팩 룰은 발견이 난 줄에, 또는 바로 윗줄에 <code>// zzop-&lt;rule-id&gt;-ok</code> 주석을 달아 인라인으로 억제할 수 있다(마커는 룰 id 에서 파생된다 — <code>float-money-compare</code> 룰은 <code>// zzop-float-money-compare-ok</code> 를 받는다). 네이티브 분석은 끄기만 가능하고, 주석으로 움직이는 예외가 둘 있다 — <code>non-idempotent-write</code>/<code>unsafe-read-endpoint</code> 는 손으로 쓴 <code>// idempotent-ok: &lt;reason&gt;</code> 를 존중하고(끝의 콜론 필수), <code>dead-candidates</code>/<code>unimported-export</code> 는 생성 파일 배너를 단 파일을 건너뛴다. 모든 룰과 네이티브 분석 id 는 실행 단위로 끌 수도 있다 — <code>zzop.config.jsonc</code> 의 <code>rules: { "&lt;id&gt;": "off" }</code>, 또는 임베더용 <code>disabledRules</code>.`,
      en: `This page is transcribed from <code>docs/rules/catalog.md</code> by
      <code>scripts/gen-site-rules.mjs</code> — that file is also, byte for byte, the
      <code>rule-catalog</code> contract document an agent reads — and a meta-test (<code>crates/engine/tests/rule_contracts/</code>) machine-checks
      that every id listed below matches what the engine actually loads at runtime, so the catalog cannot
      silently drift out of sync with the code. Each DSL pack rule below is suppressible inline with a <code>// zzop-&lt;rule-id&gt;-ok</code>
      comment on, or directly above, the finding's line (the marker is derived from the rule id — rule
      <code>float-money-compare</code> takes <code>// zzop-float-money-compare-ok</code>); native analyses are
      disable-only, with two comment-driven exceptions —
      <code>non-idempotent-write</code>/<code>unsafe-read-endpoint</code> honor a hand-written
      <code>// idempotent-ok: &lt;reason&gt;</code> (trailing colon required), and
      <code>dead-candidates</code>/<code>unimported-export</code> skip files carrying a generated-file banner.
      Every rule and
      native analysis id can also be turned off per-run instead — in <code>zzop.config.jsonc</code> via
      <code>rules: { "&lt;id&gt;": "off" }</code>, or via <code>disabledRules</code> for embedders.`,
    },

    p3: {
      ko: `<strong>범위: 아래 모든 표는 바이너리가 기본으로 싣는 룰이고, 그것이 이 레포가 싣는 룰 전부는 아니다.</strong> <code>examples/packs/</code> 에는 <em>내보낸</em> 팩이 있다 — 진짜이고, 테스트되고, 축을 선언하는 룰인데 일부러 기본 세트에 컴파일하지 않은 것들이라 아래에 행이 없다. 이름이 나오는 경우가 있다면 그건 번들 룰의 행이 그쪽을 가리키는 것이다. (<code>ls examples/packs/*.json</code> 이 명부이고, <code>docs/rules/catalog.md</code> § <em>Exported packs</em> 가 어떤 테스트가 각 팩을 내보냈는지와 그 안의 룰을 세는 커맨드를 든다.) 내보냈다는 것은 지웠다는 뜻이 아니다: 각 팩은 <strong>본문이 곧 팩 JSON</strong>인 계약 문서로 제공된다 — MCP 호스트에서는 리소스 <code>zzop://contract/example-pack-&lt;stem&gt;</code>, CLI 바이너리로는 <code>zzop contract example-pack-&lt;stem&gt;</code>(계약 인덱스가 내보낸 팩마다 한 항목씩 든다). 트리의 <code>zzop/rules/</code> — 기본 저작 팩 위치 — 아래에 하나 쓰면 다음 실행이 싣는다. config 키가 필요 없다.`,
      en: `<strong>Scope: every table below is a rule the binary loads by default, and that is not every rule
      this repository ships.</strong> <code>examples/packs/</code> holds the <em>exported</em> packs — real,
      tested, axis-declaring rules that are deliberately not compiled into the default set, so none of them
      has a row below; where one is named at all, it is a bundled rule's row pointing across at it.
      (<code>ls examples/packs/*.json</code> is the roster;
      <code>docs/rules/catalog.md</code> § <em>Exported packs</em> states which test moved each pack out and
      carries the command that counts the rules in them). Exported is not deleted: each pack is served as a
      contract document whose text IS the pack JSON — MCP resource
      <code>zzop://contract/example-pack-&lt;stem&gt;</code> on MCP hosts
      (<code>zzop contract example-pack-&lt;stem&gt;</code> with the CLI binary; the contract index lists one
      entry per exported pack). Write one under a tree's <code>zzop/rules/</code> — the default authored-pack
      location — and the next run loads it, no config key needed.`,
    },

    p4: {
      ko: `<strong>어떤 룰이 어떤 언어에 닿는지는 룰마다 다른 사실</strong>이고, 그 룰 자신의 <code>file_pattern</code> 이 정하지 팩 수준에서 정하지 않는다. 그래서 한 팩이 어떤 언어에는 빽빽하고 다른 언어에는 비어 있을 수 있다. 네이티브로 파싱하는 모든 언어는 오늘 최소 한 룰에 닿는다 — C# 도 포함이고 여러 룰에 닿는다(call-scan 해시 룰들은 C# 자신의 어휘 <code>MD5.Create</code>/<code>HashAlgorithm.Create</code> 를 말하고, 다른 룰들은 경로 후보로 <code>.cs</code> 를 받는다) — 그러나 분포는 매우 고르지 않고 언어별 합계는 발행하지 않는다. 그 수가 잘 정의되지 않기 때문이다: 어떤 패턴은 확장자뿐 아니라 디렉터리로도 좁혀지므로 한 레포 안의 <code>.ts</code> 파일 둘이 서로 다른 수에 해당한다. 대신 <strong>구체적인 경로</strong>를 물어라 — <code>docs/rules/catalog.md</code> 가 출하된 팩에 대고 그것을 답하는 커맨드 하나를 든다.`,
      en: `<strong>Which languages a rule reaches is a per-rule fact</strong>, decided by that rule's own
      <code>file_pattern</code> and nothing at the pack level, so a pack can be dense for one language and
      empty for another. Every natively parsed language is reached by at least one rule today — C# included, and by
      several rules (the call-scan hash rules speak C#'s own vocabulary, <code>MD5.Create</code>/
      <code>HashAlgorithm.Create</code>; others admit <code>.cs</code> by path candidacy) — but the distribution is very uneven and no per-language
      total is published, because the number is not well defined: some patterns are directory-scoped as
      well as extension-scoped, so two <code>.ts</code> files in one repo are eligible for different
      counts. Ask about a concrete PATH instead — <code>docs/rules/catalog.md</code> carries the one
      command that answers it against the shipped packs.`,
    },

    p5: {
      ko: `<strong>Rust 에서는 테스트 영역 안의 발견이 버려진다 — 자격증명 룰만 빼고.</strong> <code>#[cfg(test)]</code>/<code>#[test]</code> 로 가려진 항목 안에 떨어진 발견은 보고 전에 빼진다. 아래의 자격증명-정지 룰들은 그것을 거부하고 자기 행에 그렇게 적는다(<em>"Scans test paths too"</em>). 커밋된 키는 컴파일러가 남기든 말든 유출이기 때문이다. 그러니 이 축은 <strong>"테스트 영역은 제외, 단 자격증명은 예외"</strong>이지 "전부 제외"가 아니다. 메커니즘과 경계: <code>docs/rules/dsl-reference.md</code>.`,
      en: `<strong>In Rust, findings inside a test region are dropped — except the credential rules.</strong>
      A finding landing inside a <code>#[cfg(test)]</code>/<code>#[test]</code>-gated item is subtracted
      before it is reported; the credential-at-rest rules below opt out and say so in their own row
      (<em>"Scans test paths too"</em>), because a committed key is leaked whether or not the compiler
      keeps it. So the axis is "test regions are excluded EXCEPT for credentials at rest", never
      "everything is excluded". Mechanism and boundaries:
      <code>docs/rules/dsl-reference.md</code>.`,
    },

    h26: {
      ko: `<code>db</code>`,
      en: `<code>db</code>`,
    },

    h27: {
      ko: `<code>reliability</code>`,
      en: `<code>reliability</code>`,
    },

    h28: {
      ko: `<code>security</code>`,
      en: `<code>security</code>`,
    },

    h29: {
      ko: `<code>browser</code>`,
      en: `<code>browser</code>`,
    },

    h210: {
      ko: `<code>egress</code>`,
      en: `<code>egress</code>`,
    },

    h211: {
      ko: `<code>go</code>`,
      en: `<code>go</code>`,
    },

    h212: {
      ko: `<code>http</code>`,
      en: `<code>http</code>`,
    },

    h213: {
      ko: `<code>perf</code>`,
      en: `<code>perf</code>`,
    },

    h214: {
      ko: `<code>react</code>`,
      en: `<code>react</code>`,
    },

    h215: {
      ko: `<code>redis</code>`,
      en: `<code>redis</code>`,
    },

    h216: {
      ko: `<code>sql</code>`,
      en: `<code>sql</code>`,
    },

    h217: {
      ko: `네이티브 분석`,
      en: `Native analyses`,
    },

    p18: {
      ko: `전-그래프·전-레포 분석이다. 이 id 들은 DSL 룰과 같은 <code>RuleConfig</code> 활성화/심각도/억제 표면에 함께 붙는다. 각 id 는 그것을 소유한 크레이트의 <code>register_native_analyses</code> 가 등록한다 — 커널(<code>crates/core</code>)은 하나도 등록하지 않고 룰 어휘로부터 자유롭게 남는다. 다섯 크레이트가 나눠 등록하며 각자 주제 하나를 소유한다: <code>rules/native/rules-graph</code> 는 의존/데드코드 그래프 룰과 콜그래프 순수성 감사, <code>rules/native/rules-http</code> 는 단일 트리 HTTP/라우트 룰, <code>rules/native/rules-cross-layer</code> 는 <code>cross-layer/*</code> 다중 트리 조인 룰, <code>rules/native/rules-schema</code> 는 스키마 룰, <code>crates/metrics</code> 는 점수 계산. <strong>어느 id 를 누가 소유하는지는 이 문단이 아니라 표가 답한다</strong> — 명부는 그 크레이트들 각각의 <code>register_native_analyses</code> 목록이고 아래 표가 같은 집합을 행마다 든다. 여기에 id 목록을 다시 적지 않는 것은 의도다: 이 문장이 대체한 손으로 쓴 목록은 자기가 소개하는 표보다 id 하나가 모자란 채 낡아 있었고, 손으로 다시 세야 하는 목록은 위장한 census 다. 표가 스스로 다 말하지 못하는 모양이 둘 있다: <code>schema-structural</code>/<code>schema-usage</code> 는 그 아래로 보고되는 12개 <code>schema/*</code> 개별 id 에 대한 <strong>패밀리 게이트</strong>다 — 패밀리를 끄면 그 패스 전체가 꺼지고 각 <code>schema/*</code> id 는 자기가 이름 댄 룰만 끈다. 둘 다 존중되고 모든 발견의 메시지가 둘 다 말한다. 그리고 <code>crates/metrics</code> 의 id 들은 발견을 내는 룰이 아니라 점수 계산인데 같은 토글/게이팅 표면에 얹혀 있을 뿐이다. 그 metrics id 들은 <strong>심각도가 아예 없다</strong> — 등록 방식 때문이 아니라(모든 네이티브 id 가 같은 스텁을 지난다) 심각도는 발견을 매기는 것인데 이들은 발견을 안 내기 때문이다. 그래서 "Default severity" 칸이 그들에게는 <code>n/a</code> 이고, 발견이 아닌 출력에 등급을 지어내지 않는다. 각 행은 대신 자기가 내는 것을 어떤 출하 표면이 싣는지를 적는다. <code>zzop_engine::register_all_native</code> 가 다섯을 조립한다.`,
      en: `Whole-graph/whole-repo analyses. Their ids join the same <code>RuleConfig</code>
      enable/severity/suppression surface as DSL rules. Each id is
      registered by its owning crate's own <code>register_native_analyses</code> — the kernel
      (<code>crates/core</code>) itself registers none, staying rule-vocabulary-free.
      Five crates register between them, each owning one subject:
      <code>rules/native/rules-graph</code> the dependency/dead-code graph rules plus the call-graph purity
      audit, <code>rules/native/rules-http</code> the single-tree HTTP/route rules,
      <code>rules/native/rules-cross-layer</code> the <code>cross-layer/*</code> multi-tree join rules,
      <code>rules/native/rules-schema</code> the schema rules, and <code>crates/metrics</code> the score
      computations. <strong>Which ids each owns is the table's answer, not this paragraph's</strong> — the
      roster is the <code>register_native_analyses</code> list in each of those crates, and the table below
      is the same set row by row. No id list is repeated here on purpose: the hand-written one this sentence
      replaced had drifted one id short of the table it introduces, and a list that must be re-counted by
      hand is a census in disguise. Two shapes the table does not spell out on its own:
      <code>schema-structural</code>/<code>schema-usage</code> are FAMILY gates over the 12
      <code>schema/*</code> per-issue ids they report under — disabling a family switches its whole pass
      off while each <code>schema/*</code> id disables exactly the rule it names, both are honored, and
      every finding's message states both; and the <code>crates/metrics</code> ids are score computations,
      not findings-producing rules, that merely ride the same toggle/gating surface. Those metrics ids
      carry <strong>no severity at all</strong> &mdash; not because of how they
      register (every native id goes through the same stub), but because severity grades a finding and these
      emit none. The "Default severity" column therefore reads <code>n/a</code> for them rather than
      inventing a level for output that is never a finding. Each row says instead whether any shipped
      surface carries what it produces. <code>zzop_engine::register_all_native</code> composes the five.`,
    },

    p19: {
      ko: `<code>cross-layer/*</code> id 들은 다중 트리 예외다: 이들은 <code>zzop_engine::analyze_trees</code> 가 조인한 <code>CrossLayerResult</code> 위에서 돈다(여기 다른 모든 행은 트리 단위로 돈다). <code>analyzeTrees</code> 출력에서 <code>crossLayer</code> 옆의 <code>crossLayerFindings</code> 로 나간다. 이들 중 어느 것도 인라인 억제 마커를 존중하지 않는다 — 끄기 전용이다: config 의 <code>rules: { "&lt;id&gt;": "off" }</code>, 또는 임베더용 <code>disabledRules</code>.`,
      en: `The <code>cross-layer/*</code> ids are the multi-tree exception: they run over
      <code>zzop_engine::analyze_trees</code>'s joined <code>CrossLayerResult</code> (every other row here
      runs per-tree), exposed as <code>crossLayerFindings</code> alongside <code>crossLayer</code> in
      <code>analyzeTrees</code>'s output. None of them honor an inline suppression marker — they are
      disable-only: <code>rules: { "&lt;id&gt;": "off" }</code> in config, or <code>disabledRules</code>
      for embedders.`,
    },

    p20: {
      ko: `아직 없는 것: 아키텍처 룰(레이어 위반, feature-envy — 아직 어떤 크레이트도 준비돼 있지 않다), 인지/중첩 루프 복잡도 점수, 정밀한 <code>taint-flow</code> 데이터플로(오늘의 <code>security/taint-flow</code> 는 문서화된 거친 v1 공존 검사다), 인증 상태기계 분석, 추가 크로스파일 HTTP 그래프 검사(API 변화, 프론트/백엔드 스펙 드리프트), JSX/React 구조 룰 팩, env/i18n 동기화 검사. 각각 DSL 이 표현 못 하는 전-그래프 조인이거나 진짜 AST/JSX 모양을 요구한다. (Raw-Worker 라우트 추출 — 프레임워크 없는 Workers/Node 서버의 수동 <code>url.pathname</code> 디스패치 — 는 파서의 <code>pathname-dispatch</code> provide 어휘로 출하됐다.)`,
      en: `Not yet implemented: architecture rules (layer-violations, feature-envy — no crate is scaffolded for
      them yet), cognitive/nested-loop complexity scoring, precise <code>taint-flow</code> dataflow (today's
      <code>security/taint-flow</code> is a documented coarse v1 co-occurrence check), an auth-state-machine
      analysis, additional cross-file HTTP graph checks (API churn, frontend/backend spec drift), a
      JSX/React structural rule pack, and env/i18n sync checks — each needs either
      a whole-graph join the DSL can't express or real AST/JSX shape. (Raw-Worker route extraction —
      manual <code>url.pathname</code> dispatch in framework-less Workers/Node servers — shipped as the
      parser's <code>pathname-dispatch</code> provide vocabulary.)`,
    },

    h221: {
      ko: `매처`,
      en: `Matchers`,
    },

    p22: {
      ko: `모든 DSL 룰은 아래 표에서 고른 매처 모양을 <strong>정확히 하나</strong> 선언한다. 필드별 상세 의미는 DSL 레퍼런스에 있고, 여기는 요약이다.`,
      en: `Every DSL rule declares exactly one matcher shape, drawn from the table below. Full
      field-by-field semantics are in the DSL reference; this is the short version.`,
    },

    p23: {
      ko: `하나만 빼고 전부 한 파일의 <code>SourceFile</code> 조각만 보고 돌며 두 번째 파일의 내용을 볼 수 없다. <code>io-scan</code> 이 그 예외로, 파일들에 걸쳐 이미 조립된 IO 사실 위에서 전-트리로 평가한다. 룰의 인라인 억제 마커는 id 에서 <strong>파생</strong>되고(<code>zzop-&lt;rule id&gt;-ok</code>, 필드로 쓰는 것이 아니다) <code>symbol-scan</code> 을 뺀 모든 매처의 발견에 적용된다: 발견이 난 줄이나 바로 윗줄의 <code>// zzop-&lt;rule id&gt;-ok</code> 주석이 그것을 억제한다. (<code>io-scan</code>·<code>call-scan</code>·<code>literal-scan</code> 은 구조상 다언어라 <code>//</code> 뿐 아니라 <code>#</code> 주석 머리도 존중한다.) 마커에는 팩 접두사가 없다(<code>security/hardcoded-secret</code> → <code>// zzop-hardcoded-secret-ok</code>). <code>symbol-scan</code> 발견은 억제 주석을 걸 소스 줄 개념이 없어서 인라인 마커를 안 갖는다. 매처 필드 표 전체는 레포의 <a href="https://github.com/eezz4/zzop/blob/main/docs/rules/dsl-reference.md">dsl-reference.md</a> 를 보라.`,
      en: `All but one operate on a single file's <code>SourceFile</code> slice in isolation and cannot see a
      second file's content; <code>io-scan</code> is the exception, evaluating whole-tree over IO facts already
      composed across files. A rule's inline suppress marker is DERIVED from its id — <code>zzop-&lt;rule id&gt;-ok</code>,
      never authored as a field — and applies to the findings of every matcher except <code>symbol-scan</code>:
      a <code>// zzop-&lt;rule id&gt;-ok</code> comment on the finding's own line, or
      the single line directly above it, suppresses it. (<code>io-scan</code>, <code>call-scan</code> and
      <code>literal-scan</code> are multi-language by construction, so they honor a <code>#</code> comment
      leader as well as <code>//</code>.) The marker carries no pack prefix
      (<code>security/hardcoded-secret</code> → <code>// zzop-hardcoded-secret-ok</code>). <code>symbol-scan</code>
      findings have no source-line concept to anchor a suppress comment against, so they carry no inline
      marker. See <a href="https://github.com/eezz4/zzop/blob/main/docs/rules/dsl-reference.md">dsl-reference.md</a>
      in the repo for the full matcher field tables.`,
    },

    h224: {
      ko: `직접 팩 쓰기`,
      en: `Write your own pack`,
    },

    p25: {
      ko: `팩은 설정된 팩 디렉터리에서 로드되는 <code>&lt;id&gt;.json</code> 파일 하나다 — 인터프리터 수준에는 1st party/3rd party 구분이 없다. 그 디렉터리를 가리키는 철자가 둘이고 서로 바꿔 쓸 수 없다: <code>zzop.config.jsonc</code> 의 <code>packs.extraDirs</code>, 또는 임베더 요청 객체의 <code>packsDir</code>. <code>zzop/rules/</code> 디렉터리는 <code>packs.extraDirs</code> 가 이름 대지 않아도 주워진다. 아래는 작은 <code>line-scan</code> 룰 예시로, config/env 에서 와야 할 디버그 헤더 값이 하드코딩된 것을 잡는다:`,
      en: `A pack is one <code>&lt;id&gt;.json</code> file loaded from a configured packs directory — no
      first-party/third-party distinction exists at the interpreter level. Two spellings name that
      directory and they are not interchangeable: <code>packs.extraDirs</code> in a
      <code>zzop.config.jsonc</code>, or <code>packsDir</code> on an embedder's request object. A
      <code>zzop/rules/</code> directory is also picked up without <code>packs.extraDirs</code> naming it.
      Here's a small <code>line-scan</code> rule, flagging a hardcoded debug header value that should come
      from config/env instead:`,
    },

    p26: {
      ko: `두 철자 모두 디렉터리 하나 또는 배열을 받는다. 각각 독립적으로 로드된 뒤 팩 <code>id</code> 로 병합된다: 같은 <code>id</code> 가 둘 이상의 디렉터리에 나오면 목록에서 <strong>뒤쪽</strong> 디렉터리의 팩이 앞쪽을 통째로 대체한다(룰 단위 병합이 아니다). 이것이 호출자가 번들 팩 옆에 팩을 더하거나, 번들 팩을 통째로 덮어쓰는 방법이다 — 엔진을 포크하지 않고.`,
      en: `Both spellings accept either one directory or an array of directories. Each is loaded
      independently and then merged by pack <code>id</code>: if the same <code>id</code> shows up in more
      than one directory, the pack from the later directory in the list replaces the earlier one whole (not
      a per-rule merge) — this is how a caller adds packs alongside the bundled ones, or overrides a bundled
      pack outright, without forking the engine.`,
    },

    p27: {
      ko: `<strong>메시지는 원인과 처방을 쓰고 거기서 멈춘다.</strong> 억제 마커도, 룰 끄는 법도 안 적은 것을 보라: 엔진이 런타임에 두 문장을 모든 발견에 붙인다 — 마커(<code>zzop-&lt;id&gt;-ok</code>, id 에서 파생되고 그 매처 종류가 실제로 존중하는 주석 머리로 적힌다)와 <code>rules: { "&lt;pack&gt;/&lt;rule&gt;": "off" }</code> 끄기 안내. 둘 중 하나를 직접 쓰면 두 번 나오고, 손으로 쓴 마커 문장은 매처 종류가 바뀌는 순간 낡는다 — 엔진이 더는 존중하지 않는 주석 머리를 이름 대고 있게 되기 때문이다.`,
      en: `<strong>The message writes the cause and the fix, and stops there.</strong> Notice it names no
      suppress marker and no way to disable the rule: the engine appends both sentences to every finding at
      runtime — the marker (<code>zzop-&lt;id&gt;-ok</code>, derived from the id, spelled with the comment
      leaders that matcher kind actually honours) and the <code>rules: { "&lt;pack&gt;/&lt;rule&gt;": "off" }</code>
      disable hint. Writing either one yourself renders it twice, and a hand-written marker sentence goes
      stale the moment the matcher kind changes, because it names leaders the engine no longer honours.`,
    },

    p28: {
      ko: `모든 탐지가 위 매처에 맞지는 않는다. 검사가 선언→사용 추적(선언됐지만 한 번도 읽히지 않는 식별자), 크로스파일 조인(다른 파일에 정의된 상수나 라우트 핸들러를 푸는 것), 콜그래프 BFS(<em>"핸들러 X 가, 또는 그것이 전이적으로 부르는 무언가가, Y 를 한다"</em>), 또는 텍스트 공존이 아니라 진짜 AST/JSX 모양을 요구하면 네이티브 룰로 가라 — 순환/인지 복잡도와 JSX 구조 검사는 줄 단위 정규식으로 정직하게 인코딩할 방법이 없다.`,
      en: `Not every detection fits the matchers above. Reach for a native rule instead when the check
      needs declaration→use tracking (an identifier declared but never read), a cross-file join (resolving a
      constant or route handler defined in another file), call-graph BFS ("handler X, or something it calls
      transitively, does Y"), or real AST/JSX shape rather than text co-occurrence — cyclomatic/cognitive
      complexity and JSX-structural checks have no honest regex-over-lines encoding.`,
    },

  },
};
