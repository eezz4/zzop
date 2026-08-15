// 개요 — Overview
// 문장은 여기 한 번만 산다. HTML 은 scripts/site/render.mjs 가 만든다.

export default {
  bands: [
    {
      header: true,
      blocks: [
        ["eyebrow", "Zero Zone Of Pain"],
        [
          "h1",
          {
            ko: "에이전트가 못 읽는 코드베이스를,<br>zzop 이 읽는다.",
            en: "Your agent can't read the whole repo.<br>zzop reads it.",
          },
        ],
        [
          "lede",
          {
            ko: `AI 코딩 에이전트는 저장소 전체를 컨텍스트에 못 담는다. 안 읽은 것은 추측한다.
      zzop 은 저장소를 읽어 <strong>한 장의 JSON 지도</strong>로 답한다 —
      어떤 호출이 어떤 라우트에 닿고, 어떤 호출은 아무 데도 안 닿는지.
      그리고 <em>같은 입력이면 매번 같은 답</em>을 준다.`,
            en: `A coding agent can't fit your repository in context, and what it doesn't read, it guesses.
      zzop reads it and answers with <strong>one JSON map</strong> — which calls reach which routes,
      and which reach nothing at all. <em>Same input, same answer, every time.</em>`,
          },
        ],
        [
          "muted",
          {
            ko: "코드는 쓰지 않는다. 에이전트가 딛고 일하는 <strong>이해</strong>를 정확하게 만든다.",
            en: "It writes no code. It makes the <strong>understanding</strong> your agent works from accurate.",
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "문제", en: "The problem" }],
        [
          "h2",
          {
            ko: "라우트 하나를 고쳤다. 아무도 눈치채지 못했다.",
            en: "One route was renamed. Nothing noticed.",
          },
        ],
        [
          "p",
          {
            ko: `독립적으로 만들어진 두 앱 — React 프론트엔드와 Express 백엔드. 코드도 타입도 공유하지 않는다.
      백엔드 라우트 이름을 하나 정리한다.`,
            en: `Two independently authored apps — a React frontend, an Express backend. They share no code and no types.
      Someone tidies up one backend route.`,
          },
        ],
        [
          "panel",
          {
            tab: { ko: "백엔드 정리", en: "The backend tidy-up" },
            lines: [
              `<span class="del">- router.put('/user',      auth.required, …)</span>`,
              `<span class="add">+ router.put('/users/me',  auth.required, …)</span>`,
            ],
          },
        ],
        [
          "p",
          {
            ko: `프론트엔드 빌드는 <strong>깨끗하다.</strong> 라우트가 문자열 리터럴이라 대조할 타입이 없다.
      자체 목킹 테스트도 <strong>초록이다.</strong>
      계약은 이미 깨졌는데, 한 저장소만 보는 린터·타입체커·테스트는 <strong>구조적으로 못 본다</strong> —
      증거가 두 저장소에 나뉘어 있고 컴파일러 경계를 넘지 않기 때문이다.`,
            en: `The frontend build stays <strong>clean</strong> — the route is a string literal, so there is no type to check it
      against — and its mocked tests stay <strong>green</strong>. The contract is already broken, and a linter, a
      type-checker or a test suite scoped to one repository is <strong>structurally unable to see it</strong>: the
      evidence is split across two repos and never crosses a compiler boundary.`,
          },
        ],
        [
          "panel",
          {
            tab: { ko: "zzop 이 답하는 것", en: "What zzop reports" },
            lines: [
              `<span class="c">=== unprovided consumes ===</span>`,
              `  "PUT /api/user"       @ fe-vite     src/pages/Settings.jsx:19   <span class="hit">←</span>`,
              {
                code: "  ",
                comment: {
                  ko: "   호출은 살아 있는데 받는 라우트가 없다",
                  en: "   the call now hits nothing",
                },
              },
              ``,
              `<span class="c">=== unconsumed provides ===</span>`,
              `  "PUT /api/users/me"   @ be-express  auth.controller.ts:61       <span class="hit">←</span>`,
              {
                code: "  ",
                comment: {
                  ko: "   라우트는 있는데 부르는 쪽이 없다",
                  en: "   the route nobody calls",
                },
              },
            ],
          },
        ],
        [
          "note",
          {
            ko: "깨진 양쪽을 파일과 줄까지. 디스크에서 아무것도 공유하지 않는 두 저장소를 가로질러서.",
            en: "Both ends of the break, located to the file and line, across two repos that share nothing on disk.",
          },
        ],
      ],
    },

    {
      wide: true,
      loose: true,
      blocks: [
        [
          "group",
          { tight: true },
          [
            ["eyebrow", { ko: "무엇을 보나", en: "What it looks at" }],
            ["h2", { ko: "엔진 하나, 렌즈 셋", en: "One engine, three lenses" }],
          ],
        ],
        [
          "lenses",
          [
            {
              h: { ko: "교차 계층", en: "Cross-layer" },
              p: {
                ko: "프론트 호출과 백엔드 라우트를 잇는다. 아무도 안 부르는 엔드포인트, 메서드 불일치, 경로 드리프트 — 저장소를 넘어서도.",
                en: "Frontend calls joined to backend routes: unconsumed endpoints, method mismatches, path drift — even across repositories.",
              },
            },
            {
              h: { ko: "보안", en: "Security" },
              p: {
                ko: "SQL 인젝션, 약한 해시, SSRF, 하드코딩된 시크릿. DSL 룰과 네이티브 분석이 언어를 가로질러 본다.",
                en: "SQL injection, weak hashing, SSRF, hardcoded secrets — DSL rules plus native analyses, across languages.",
              },
            },
            {
              h: { ko: "구조", en: "Structure" },
              p: {
                ko: "순환 의존, 죽은 코드, 리팩터 우선순위. 구조적 부채를 파일 단위로 셈한다.",
                en: "Circular dependencies, dead code, refactor priority — structural debt quantified per file.",
              },
            },
          ],
        ],
        [
          "muted",
          {
            ko: `네이티브로 읽는 언어는 여덟이다 — TypeScript · Python · Java · C# · Rust · Go · Prisma · SQL.
      그 밖의 언어는 어댑터로 주입한다.`,
            en: `Eight languages are parsed natively — TypeScript · Python · Java · C# · Rust · Go · Prisma · SQL.
      Anything else joins through an adapter.`,
          },
          { inner: true },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "왜 믿나", en: "Why trust it" }],
        [
          "h2",
          { ko: "못 본 것을 스스로 말한다.", en: "It tells you what it could not see." },
        ],
        [
          "p",
          {
            ko: `정적 분석의 진짜 위험은 틀린 답이 아니라 <strong>침묵</strong>이다.
      아무것도 안 나온 것과 못 본 것을 구별할 수 없으면, 초록은 아무 뜻도 없다.`,
            en: `The real hazard in static analysis isn't a wrong answer — it's <strong>silence</strong>.
      If "found nothing" and "couldn't look" are indistinguishable, green means nothing.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "결정적", en: "Deterministic" },
              v: {
                ko: "같은 입력이면 <strong>바이트까지 같은 출력</strong>. 타임스탬프도, 흔들리는 정렬도 없다.",
                en: "Same input, <strong>byte-identical output</strong>. No timestamps, no unstable ordering.",
              },
            },
            {
              k: { ko: "빈 배열의 뜻", en: "An empty array" },
              v: {
                ko: "언제나 <strong>“봤고, 없었다”</strong>. 못 본 것은 빈 배열이 아니라 부재로 보고된다.",
                en: "Always means <strong>analyzed, found nothing</strong>. What it couldn't see is reported as absent, never as empty.",
              },
            },
            {
              k: { ko: "못 한 일", en: "What it couldn't do" },
              v: {
                ko: "이번 실행이 제공하지 못한 능력은 <code>warnings</code> 에 스스로 적힌다. 대충 채워 넣지 않는다.",
                en: "A capability this run couldn't provide is self-reported in <code>warnings</code>, never stubbed.",
              },
            },
            {
              k: { ko: "발견마다", en: "Every finding" },
              v: {
                ko: "그것을 끄는 <strong>정확한 설정</strong>을 같이 준다. 룰 발견은 고치는 법과 억제 주석까지.",
                en: "Names the <strong>exact config</strong> that silences it — rule findings add the concrete fix and a suppress marker.",
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `그래서 zzop 은 목록을 주는 도구가 아니라 <strong>방향을 주는 도구</strong>다.
      결과는 리팩터 ROI 로 정렬되고, 두 저장소가 조용히 어긋난 것은 일급 발견이 된다.`,
            en: `That is what separates it from a flat list of findings: results are ranked by refactor ROI, and
      "two codebases quietly disagree" becomes a first-class finding.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "실전에서", en: "In the field" }],
        [
          "h2",
          {
            ko: "X 가 공개한 코드 전부를 걸어 봤다.",
            en: "Run over everything X ever open-sourced.",
          },
        ],
        [
          "p",
          {
            ko: `X(구 트위터)와 xAI 가 공개한 저장소 12개 — For You 피드부터 Grok 의 빌드 시스템까지 —
      를 한 번에 걸었다. 아래 수는 2026-08-15 에 zzop 0.31.0 으로 잰 것이고,
      <strong>수마다 무엇을 센 것인지가 다르다</strong>: <em>walked</em> 는 트리에서 걸은 파일,
      <em>파서 수신</em>은 그중 네이티브 파서 8종이 실제로 받은 것, 심볼은 그 파서들이 추출한 선언이다.`,
            en: `Twelve repositories X (formerly Twitter) and xAI have open-sourced — from the For You feed
      to Grok's build system — in one run. The numbers below were measured 2026-08-15 with zzop 0.31.0,
      and <strong>each counts a different thing</strong>: <em>walked</em> is files visited in the tree,
      <em>dispatched</em> is the subset the eight native parsers actually received, and symbols are the
      declarations those parsers extracted.`,
          },
        ],
        [
          "vs",
          [
            {
              k: "x-algorithm",
              v: {
                ko: `X 의 For You 피드(Rust + Python). <strong>215파일 중 207을 파서가 받았다 — 96%</strong>.
      이 세트에서 가장 높고, 이유도 정직하다: Rust 와 Python 은 정면 커버리지다.
      한계도 같은 런이 말했다 — 실서비스 경로는 gRPC 인데 zzop 에 gRPC 인식기가 없어서, axum 을 임포트하고도 라우트 0 이 <em>맞는 답</em>이다.`,
                en: `X's For You feed (Rust + Python). <strong>207 of 215 files dispatched to a parser — 96%</strong>,
      the highest in the set, for an honest reason: Rust and Python are head-on coverage.
      The same run stated the limit too — the service speaks gRPC, which zzop has no recognizer for,
      so a file importing axum with zero routes is the <em>correct</em> answer, and the run says so.`,
              },
            },
            {
              k: "grok-build",
              v: {
                ko: `Grok 의 빌드 시스템. 단일 트리에서 <strong>심볼 71,142</strong> —
      zzop 자기 저장소(13,259)의 5.4배를 한 트리가 낸다.`,
                en: `Grok's build system: <strong>71,142 symbols from a single tree</strong> —
      5.4× what zzop's own repository (13,259) yields.`,
              },
            },
            {
              k: { ko: "12트리 전체", en: "All twelve trees" },
              v: {
                ko: `walked 12,078 파일, 그중 파서 수신 <strong>4,480(37%)</strong> — 나머지는 대부분 지원 밖 언어(Scala 등)이고,
      그 파일들도 버려지지 않고 줄 수와 텍스트 룰은 받는다. 심볼 합 <strong>94,281</strong>.
      전량 <code>facts</code> 한 번이 콜드 73초, 캐시 뒤 30초.`,
                en: `12,078 files walked, <strong>4,480 of them dispatched (37%)</strong> — the rest are mostly
      languages outside the eight (Scala above all), and even those still get line counts and text rules.
      <strong>94,281 symbols</strong> in total. One <code>facts</code> run over everything: 73s cold, 30s warm.`,
              },
            },
          ],
          { wide: true },
        ],
        [
          "note",
          {
            ko: `두 비율을 섞지 않는 것이 이 표의 요점이다: 96% 는 <strong>한 트리</strong>(x-algorithm)의 수이고
      37% 가 <strong>세트 전체</strong>다. 낮은 쪽을 숨기면 높은 쪽도 못 믿게 된다.
      재는 법: 저장소들을 클론하고 <code>zzop facts --config</code> 한 번 — 트리마다 <code>coverage</code> 블록이
      자기 파일·수신·심볼 수를 내고, <strong>열두 블록을 더하면</strong> 위 합계가 나온다.
      초 단위는 그 실행의 벽시계 시간이지 출력 필드가 아니다.`,
            en: `The point of this table is refusing to blend two ratios: 96% belongs to <strong>one tree</strong>
      (x-algorithm), 37% to <strong>the whole set</strong>. Hide the low one and the high one stops being credible.
      To re-measure: clone the repositories and run <code>zzop facts --config</code> once — each tree's
      <code>coverage</code> block prints its own file / dispatched / symbol counts, and <strong>the twelve
      blocks sum</strong> to the totals above. The seconds are that run's wall clock, not a printed field.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "그 런이 무엇을 봤나", en: "What that run saw" }],
        [
          "h2",
          {
            ko: "결함 171건 — 그리고 zzop 이 그걸 어떻게 말하는지.",
            en: "171 findings — and how zzop states each one.",
          },
        ],
        [
          "p",
          {
            ko: `같은 저장소 12개에 룰을 걸었다(2026-08-15, zzop 0.32.0). 결함이 나온 곳은 <strong>4개뿐</strong>이고
      <strong>8개는 0건</strong>이다 — 0건도 결과라, 숨기지 않고 공시한다. 합 <strong>171건</strong>:
      심각도로 나누면 <strong>critical 5 · warning 122 · info 44</strong>. 여기서 핵심은
      <strong>심각도가 취약점 판정이 아니라 렉시컬 판정</strong>이라는 것이다.`,
            en: `The same twelve repositories, run against the rule packs (2026-08-15, zzop 0.32.0). Only
      <strong>four</strong> produced any finding; <strong>eight came back with zero</strong> — a zero is a
      result too, disclosed rather than hidden. <strong>171 in total</strong>, split
      <strong>5 critical · 122 warning · 44 info</strong>. The point that makes the number honest:
      <strong>severity is a lexical judgment, not a vulnerability verdict</strong>.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "critical 5건 — 전부 테스트 안", en: "5 critical — all inside tests" },
              v: {
                ko: `다섯 건 모두 <code>conn-string-credentials</code>(<code>scheme://user:pass@host</code> 를
      소스에 박은 URL)이고, 다섯 건 모두 <strong><code>#[test]</code> 함수 안</strong>이다 —
      자격증명을 <em>지우는</em> 코드의 테스트 입력(<code>strip_url_credentials_removes_token</code>)이다.
      zzop 은 이걸 취약점이라 부르지 않는다. 렉시컬로 보이는 것을 보고하고, 잠재우는 config 키를 문장에 담고,
      사람이 5초 만에 픽스처임을 읽게 둔다.`,
                en: `All five are <code>conn-string-credentials</code> (a URL embedding
      <code>scheme://user:pass@host</code> in source), and all five sit <strong>inside a
      <code>#[test]</code> function</strong> — test inputs for the code that <em>strips</em> credentials
      (<code>strip_url_credentials_removes_token</code>). zzop never calls them vulnerabilities. It reports
      what it lexically sees, names the config key that silences it, and lets a human read them as fixtures
      in five seconds.`,
              },
            },
            {
              k: { ko: "진짜 신호는 warning 에", en: "The real signal is in the warnings" },
              v: {
                ko: `가장 많이 뜬 룰: <code>command-and-interpolation</code> 33 · <code>reqwest-no-timeout</code> 24 ·
      <code>hardcoded-secret</code> 19 · <code>high-entropy-secret</code> 13 · <code>fs-check-then-use</code> 9.
      크로스-레인은 별도로, 소비되지 않는 엔드포인트·제공자 없는 호출 같은 계약 틈 20건을 냈다 — 파일 하나를 보는 룰이
      못 보는 층이다.`,
                en: `Top rules by count: <code>command-and-interpolation</code> 33 · <code>reqwest-no-timeout</code> 24 ·
      <code>hardcoded-secret</code> 19 · <code>high-entropy-secret</code> 13 · <code>fs-check-then-use</code> 9.
      The cross-layer join added 20 more — contract gaps like unconsumed endpoints and unprovided calls, the
      layer a single-file rule cannot see.`,
              },
            },
            {
              k: { ko: "8개는 0건 — 그것도 공시", en: "Eight clean — that's disclosed too" },
              v: {
                ko: `communitynotes·grok-1·xai-proto·xai-sdk-python·x-algorithm 등 8개는 결함 0.
      단 zzop 은 "0 = 안전"이라고 말하지 않는다: 같은 런이 트리마다 <em>무엇을 못 봤는지</em>(지원 밖 언어, 미해석 핸들러,
      gRPC 처럼 인식기 없는 경로)를 커버리지 블록에 담아, 0건이 <em>깨끗</em>인지 <em>범위 밖</em>인지 읽게 한다.`,
                en: `Eight — communitynotes, grok-1, xai-proto, xai-sdk-python, x-algorithm and more — came back
      with zero. zzop does not read a zero as "safe": the same run records, per tree, <em>what it could not
      see</em> (languages outside the eight, unresolved handlers, recognizer-less paths like gRPC) in the
      coverage block, so a zero reads as <em>clean</em> or <em>out of scope</em>, never a blank claim.`,
              },
            },
          ],
          { wide: true },
        ],
        [
          "note",
          {
            ko: `이게 제품의 논지다: zzop 은 <strong>결함을 자랑하지 않는다 — 자기가 무엇을 봤고 무엇을 못 봤는지를
      정직하게 말한다</strong>. critical 다섯이 전부 테스트 픽스처인 것이 약점이 아니라, 그걸 <em>취약점이라 우기지 않은 것</em>이
      강점이다. 재현: 저장소 12개를 클론하고 <code>zzop cross --config</code> 한 번 — 트리마다 심각도·룰별 카운트가 나오고,
      each finding 은 rule id·<code>file:line</code>·잠재우는 config 키를 함께 낸다. 이 코퍼스의 그래프 다섯 장 —
      <code>dep</code> 전량 4,457파일을 한 캔버스에 그린 것 포함 — 은 <a href="x-showcase.html">따로 한 페이지</a>에 있다.`,
            en: `This is the product's thesis: zzop <strong>does not brag about findings — it states honestly what it
      saw and what it could not</strong>. That all five criticals are test fixtures is not the weakness; the
      strength is that it <em>did not dress them as vulnerabilities</em>. Reproduce: clone the twelve
      repositories and run <code>zzop cross --config</code> once — each tree prints counts by severity and rule,
      and every finding carries a rule id, a <code>file:line</code>, and the config key that silences it. The
      five graphs of this corpus — including the full <code>dep</code> of all 4,457 files drawn on one canvas —
      are on <a href="x-showcase.html">a page of their own</a>.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "시작", en: "Start" }],
        ["h2", { ko: "세 줄이면 된다.", en: "Three lines." }],
        [
          "run",
          [
            {
              cmd: "zzop init",
              what: { ko: "설정 파일을 쓴다 · 트리당 한 번", en: "writes the config · once per tree" },
            },
            {
              cmd: "zzop analyze .",
              what: { ko: "이 트리를 분석 · JSON 출력", en: "analyze this tree · JSON out" },
            },
            {
              cmd: "zzop cross ./web ./api",
              what: { ko: "두 저장소를 잇는다", en: "join two repositories" },
            },
          ],
        ],
        [
          "muted",
          {
            ko: "Node.js 도, npm 도, 컴파일할 것도 없다. GitHub Releases 에서 바이너리를 받으면 끝이다.",
            en: "No Node.js, no npm, nothing to compile — download the binary from GitHub Releases.",
          },
        ],
        [
          "vs",
          [
            {
              k: "<code>zzop</code>",
              v: {
                ko: "터미널과 CI 용 CLI. JSON 을 stdout 으로 낸다.",
                en: "A plain CLI for a terminal or CI. JSON to stdout.",
              },
            },
            {
              k: "<code>zzop-mcp</code>",
              v: {
                ko: "에이전트용 MCP 서버. 플러그인을 깔면 <strong>당신이 커맨드를 칠 일이 없다</strong> — 에이전트가 직접 묻는다.",
                en: "An MCP server for your agent. Install the plugin and <strong>you run no commands</strong> — the agent asks.",
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `설정은 선택이 아니라 필수다. zzop 이 당신의 프로젝트에 대해 <em>추측했을</em> 이름들이 거기 산다 —
      선언하지 않은 키는 zzop 이 판정하지 않는다.`,
            en: `A config is required rather than optional: the names zzop would otherwise <em>guess</em> about your project live
      in it, and a key you don't declare is a judgment zzop doesn't make.`,
          },
        ],
      ],
    },
  ],
};
