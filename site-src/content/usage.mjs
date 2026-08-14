// 사용법 — Usage
// 문장은 여기 한 번만 산다. HTML 은 scripts/site/render.mjs 가 만든다.

export default {
  bands: [
    {
      header: true,
      blocks: [
        ["eyebrow", { ko: "사용법", en: "Usage" }],
        [
          "h1",
          {
            ko: "당신이 칠 것인가,<br>에이전트가 칠 것인가.",
            en: "Do you type the commands,<br>or does your agent?",
          },
        ],
        [
          "lede",
          {
            ko: `바이너리는 둘인데 엔진은 하나다. 그래서 고르는 기준은 <em>능력이 아니라 누가 실행하느냐</em>다.
      먼저 이걸 정하고 설치를 시작한다.`,
            en: `Two binaries, one engine — so you choose by <em>who runs it</em>, not by what it can do.
      Decide this first, then install.`,
          },
        ],
        [
          "vs",
          [
            {
              k: "<code>zzop-mcp</code>",
              v: {
                ko: "에이전트가 부른다. 플러그인이나 <code>.mcpb</code> 번들을 깔면 <strong>당신은 커맨드를 치지 않는다</strong>.",
                en: "Your agent calls it. Install the plugin or the <code>.mcpb</code> bundle and <strong>you type nothing</strong>.",
              },
            },
            {
              k: "<code>zzop</code>",
              v: {
                ko: "당신이 친다. 터미널 · CI · 스크립트. JSON 이 stdout 으로 나온다.",
                en: "You type it. Terminal, CI, a script — JSON to stdout.",
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `둘은 같은 핸들러로 디스패치한다 — 같은 경로면 같은 판정이다. 다만
      <code>manifest</code> · <code>diff</code> · <code>facts</code> · <code>coverage</code> ·
      <code>graph</code> · <code>explain</code> · <code>init</code> 은 CLI 에만 있다.
      어느 쪽도 네트워크 요청을 하지 않는다.`,
            en: `Both dispatch to the same handlers, so the same path gets the same verdict. Only these lanes are
      CLI-only: <code>manifest</code>, <code>diff</code>, <code>facts</code>, <code>coverage</code>,
      <code>graph</code>, <code>explain</code>, <code>init</code>. Neither binary makes a network request.`,
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "설치", en: "Install" }],
        ["h2", { ko: "컴파일할 것이 없다.", en: "There is nothing to compile." }],
        [
          "p",
          {
            ko: `Node.js 도 npm 도 없어도 된다. 네 갈래 중 하나를 고르면 되는데,
      앞의 둘은 <strong>에이전트 레인</strong>이고 뒤의 둘은 <strong>CLI 레인</strong>이다.`,
            en: `No Node.js, no npm. Four ways in — the first two are the <strong>agent lane</strong>,
      the last two the <strong>CLI lane</strong>.`,
          },
        ],
        [
          "vs",
          [
            {
              k: "Claude Code",
              v: {
                ko: "<code>/plugin marketplace add eezz4/zzop</code> 다음 <code>/plugin install zzop@zzop</code>. 첫 세션에는 도구가 아직 안 보인다 — <strong>한 번 재시작</strong>하면 나온다.",
                en: "<code>/plugin marketplace add eezz4/zzop</code>, then <code>/plugin install zzop@zzop</code>. The first session doesn't list the tools yet — <strong>restart once</strong>.",
              },
            },
            {
              k: "Claude Desktop",
              v: {
                ko: "<code>.mcpb</code> 번들을 끌어다 놓는다. 플랫폼 바이너리가 그 안에 들어 있다.",
                en: "Drag-and-drop the <code>.mcpb</code> bundle; it carries the platform binary.",
              },
            },
            {
              k: { ko: "릴리스", en: "Releases" },
              v: {
                ko: "GitHub Releases 에서 <code>zzop-cli-&lt;platform&gt;</code>(CLI) 또는 <code>zzop-mcp-&lt;platform&gt;</code>(MCP) 을 받아 <code>PATH</code> 에 둔다.",
                en: "Download <code>zzop-cli-&lt;platform&gt;</code> (CLI) or <code>zzop-mcp-&lt;platform&gt;</code> (MCP) from GitHub Releases and put it on <code>PATH</code>.",
              },
            },
            {
              k: "npm",
              v: {
                ko: "<code>npm i -g @zzop/cli</code> — 같은 네이티브 바이너리를 플랫폼별로 받아 띄우는 얇은 런처다.",
                en: "<code>npm i -g @zzop/cli</code> — a thin launcher that fetches and spawns this exact native binary.",
              },
            },
          ],
        ],
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
              what: {
                ko: "두 트리를 잇는다 · 각 트리에 설정이 있어야 한다",
                en: "join two trees · each needs its own config",
              },
            },
          ],
        ],
        [
          "muted",
          {
            ko: `MCP 레인이면 이 셋을 칠 일이 없다. 클라이언트가 <code>zzop-mcp mcp</code> 를 대신 띄우고, 도구는 에이전트가 부른다.`,
            en: `On the MCP lane you type none of it: the client runs <code>zzop-mcp mcp</code> for you and the agent calls the tools.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "설정", en: "Config" }],
        ["h2", { ko: "설정이 없으면 분석 자체를 거부한다.", en: "No config, no analysis." }],
        [
          "p",
          {
            ko: `zzop 은 설정 파일이 없는 트리를 <strong>분석하지 않는다</strong>. 당신이 본 적 없는 가정으로 당신 코드를 판정하지 않기 위해서다.
      <code>vocabulary</code> 블록이 그 가정의 자리다 — 무엇을 auth 가드라 부르는지, 어떤 URL 조각이 API 를 뜻하는지.
      <em>선언하지 않은 키는 zzop 이 아예 묻지 않는 질문</em>이 된다.`,
            en: `zzop <strong>refuses</strong> a tree with no config rather than judge your code under assumptions you never saw.
      The <code>vocabulary</code> block is where those assumptions live — what you call an auth guard, which URL segments
      mark your API. <em>A key you don't declare is a question zzop never asks.</em>`,
          },
        ],
        [
          "panel",
          {
            tab: "zzop.config.jsonc",
            lines: [
              `{`,
              {
                code: `  <span class="hit">"roots"</span>: ["."],                              `,
                comment: { ko: "// 분석할 트리", en: "// trees to analyze" },
              },
              {
                code: `  <span class="hit">"packs"</span>:   { "only": ["security", "sql"] },  `,
                comment: { ko: "// 주제 단위 스위치", en: "// whole packs" },
              },
              {
                code: `  <span class="hit">"rules"</span>:   { "sql/nplus1": "off" },          `,
                comment: { ko: "// 룰 하나씩", en: "// one rule at a time" },
              },
              {
                code: `  <span class="hit">"exclude"</span>: ["legacy/"],                      `,
                comment: { ko: "// 경로로 버리기", en: "// drop by path" },
              },
              `  <span class="hit">"vocabulary"</span>: {`,
              `    "skipDirs": ["node_modules", "dist", "build", ".git"]`,
              {
                code: `    `,
                comment: {
                  ko: "// 나머지 이름들은 zzop init 이 채워 준다",
                  en: "// zzop init fills in the rest",
                },
              },
              `  }`,
              `}`,
            ],
          },
        ],
        [
          "p",
          {
            ko: `직접 쓸 필요는 없다. <code>zzop init</code> 이 <strong>주석 달린 시작 파일</strong>을 써 주는데,
      값이 전부 zzop 자신의 제안이라 그 파일은 기본값을 바꾸는 게 아니라 <em>보여준다</em>.
      이미 있는 파일은 <code>--force</code> 없이 덮지 않는다.`,
            en: `You needn't hand-write it: <code>zzop init</code> writes an <strong>annotated starter file</strong> whose every value
      is zzop's own suggestion — so it <em>documents</em> the defaults instead of changing them. It never overwrites an
      existing config without <code>--force</code>.`,
          },
        ],
        [
          "note",
          {
            ko: `키 전부를 여기 늘어놓지 않는다. <code>zzop contract config-surface</code> 가 기계로 검증된 전체 키 목록을,
      <code>zzop contract config-template</code> 가 그 시작 파일을 그대로 찍는다 — 소스 체크아웃 없이 바이너리 하나로.
      입출력 JSON 계약은 <a href="#p-contract">계약</a> 페이지에 있다.`,
            en: `The full key list doesn't belong on a page: <code>zzop contract config-surface</code> prints the machine-checked
      vocabulary and <code>zzop contract config-template</code> prints that starter file — from the binary alone.
      The input/output JSON contract is on the <a href="#p-contract">Contract</a> page.`,
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "발견 끄기", en: "Silencing" }],
        [
          "h2",
          {
            ko: "끄는 법은 발견이 직접 알려준다.",
            en: "Every finding names the switch that silences it.",
          },
        ],
        [
          "p",
          {
            ko: `층은 셋이고 넓은 쪽부터 좁아진다 — <strong>팩 통째</strong>(<code>packs</code>) ·
      <strong>룰 하나</strong>(<code>rules</code>) · <strong>그 한 줄</strong>(인라인 마커).
      앞의 둘은 위 설정 블록 그대로다.`,
            en: `Three layers, widest first — a <strong>whole pack</strong> (<code>packs</code>), <strong>one rule</strong>
      (<code>rules</code>), <strong>one line</strong> (an inline marker). The first two are the config block above.`,
          },
        ],
        [
          "p",
          {
            ko: `마커는 <strong>찾아볼 필요가 없다</strong>. 룰 id 에서 팩 접두사를 떼고 <code>zzop-…-ok</code> 로 감싼 것이다.
      저장된 값이 아니라 유도된 값이라 룰 이름이 바뀌면 마커도 같이 바뀌고, 발견의 메시지가 매번 정확한 마커를 적어 준다.
      그 줄이나 바로 윗줄에 쓴다.`,
            en: `You never look a marker up: strip the pack prefix from the rule id and wrap it as <code>zzop-…-ok</code>.
      It is derived rather than stored, so renaming a rule renames its marker — and every finding's message spells the
      exact one. Put it on the flagged line or the line directly above it.`,
          },
        ],
        [
          "panel",
          {
            tab: { ko: "한 줄만 끈다", en: "Silence one line" },
            lines: [
              `<span class="c">sql/nplus1  →  zzop-nplus1-ok</span>`,
              ``,
              {
                code: `const items = list.map(x =&gt; db.find(x.id)); <span class="hit">// zzop-nplus1-ok</span>: `,
                comment: { ko: "아래에서 배치한다", en: "batched below" },
              },
            ],
          },
        ],
        [
          "p",
          {
            ko: `네이티브 분석 — <code>dead-candidates</code>, <code>cross-layer/unconsumed-endpoint</code> 같은 것들 — 에는 마커가 없다.
      설정으로만 끈다: <code>"dead-candidates": "off"</code>. 심각도만 낮추거나 경로만 빼려면 객체로 쓴다 —
      <code>{ "severity": "warn", "exclude": ["legacy/"] }</code>.`,
            en: `Native analyses — <code>dead-candidates</code>, <code>cross-layer/unconsumed-endpoint</code> and friends — carry no
      marker and are disabled in config only: <code>"dead-candidates": "off"</code>. To soften rather than silence, use the
      object form: <code>{ "severity": "warn", "exclude": ["legacy/"] }</code>.`,
          },
        ],
        [
          "note",
          {
            ko: `예외는 둘뿐이다. <code>non-idempotent-write</code> / <code>unsafe-read-endpoint</code> 는 손으로 쓴
      <code>// idempotent-ok: &lt;이유&gt;</code> 를 읽고(콜론이 필수다), <code>dead-candidates</code> /
      <code>unimported-export</code> 는 첫 8줄에 생성 파일 배너(<code>@generated</code> 등)가 있으면 그 파일을 건너뛴다.`,
            en: `Two exceptions only: <code>non-idempotent-write</code> / <code>unsafe-read-endpoint</code> honor a hand-written
      <code>// idempotent-ok: &lt;reason&gt;</code> (the colon is required), and <code>dead-candidates</code> /
      <code>unimported-export</code> skip a file whose first 8 lines carry a generated-file banner (<code>@generated</code>, …).`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "커맨드", en: "Commands" }],
        ["h2", { ko: "여섯 개면 거의 다 된다.", en: "Six commands cover most days." }],
        [
          "vs",
          [
            {
              k: "<code>zzop analyze &lt;path&gt;</code>",
              v: { ko: "한 트리를 분석한다.", en: "Analyze one tree." },
            },
            {
              k: "<code>zzop cross &lt;path&gt;...</code>",
              v: {
                ko: "트리 둘 이상을 계층 경계에서 잇는다 — zzop 의 본론.",
                en: "Join 2+ trees across the layer boundary — zzop's headline.",
              },
            },
            {
              k: "<code>zzop file &lt;path&gt; &lt;tree&gt;...</code>",
              v: {
                ko: "이 파일에 대해 아는 전부. 상한 없이, 판정 하나와 함께.",
                en: "Everything zzop knows about one file — uncapped, with one sealed verdict.",
              },
            },
            {
              k: "<code>zzop endpoint &lt;pattern&gt; &lt;path&gt;...</code>",
              v: {
                ko: "io 키 하나가 제공됐나 · 소비됐나 · 이어졌나. 버킷을 눈으로 세지 않는다.",
                en: "Is one io key provided, consumed or joined? One verdict, not buckets to eyeball.",
              },
            },
            {
              k: "<code>zzop coverage &lt;path&gt;...</code>",
              v: {
                ko: "이번 실행이 <strong>무엇을 못 봤나</strong>. 단일 점수는 일부러 없다. <em>CLI 전용.</em>",
                en: "What this run <strong>couldn't see</strong>. Deliberately no single score. <em>CLI only.</em>",
              },
            },
            {
              k: "<code>zzop explain &lt;rule-id&gt;</code>",
              v: {
                ko: "룰 하나의 데이터 — 억제 마커도 여기서 나온다. <em>CLI 전용.</em>",
                en: "One rule's own data — including its suppress marker. <em>CLI only.</em>",
              },
            },
          ],
          { wide: true },
        ],
        [
          "note",
          {
            ko: `<code>analyze</code> 와 <code>cross</code> 는 <code>--severity</code> · <code>--rule</code> · <code>--limit</code> 로 목록을 좁힌다.
      좁아지는 건 <strong>목록뿐</strong>이고 카운트는 언제나 전부를 덮으며, 잘렸다는 사실은 출력에 적힌다.
      종료 코드는 <code>0</code> 성공 · <code>1</code> 실행 실패 · <code>2</code> 인자 모양 오류 —
      심각도로 갈리는 종료 코드는 없으니 CI 게이트는 JSON 을 직접 읽어 만든다.`,
            en: `<code>analyze</code> and <code>cross</code> narrow the list with <code>--severity</code>, <code>--rule</code> and
      <code>--limit</code> — only the <strong>list</strong>; counts always cover everything and truncation is disclosed.
      Exit codes: <code>0</code> ran, <code>1</code> runtime failure, <code>2</code> bad argument shape. There is no
      severity-gated exit code, so gate CI by reading the JSON yourself.`,
          },
        ],
        [
          "muted",
          {
            ko: `전체 목록은 <code>zzop help</code> 가 답한다 — 페이지가 아니라 바이너리가 정본이다.
      그림은 <code>zzop graph</code>(<a href="#p-graph">그래프</a>), 룰 목록은 <a href="#p-rules">룰</a> 페이지에 있다.`,
            en: `<code>zzop help</code> is the full list — the binary is authoritative, not this page.
      Pictures come from <code>zzop graph</code> (<a href="#p-graph">Graph</a>); the rules live on the
      <a href="#p-rules">Rules</a> page.`,
          },
        ],
      ],
    },
  ],
};
