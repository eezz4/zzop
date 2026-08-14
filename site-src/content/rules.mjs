// 룰 — Rules
// 문장은 여기 한 번만 산다. HTML 은 scripts/site/render.mjs 가 만든다.

export default {
  bands: [
    {
      header: true,
      blocks: [
        ["eyebrow", { ko: "룰", en: "Rules" }],
        [
          "h1",
          {
            ko: "176개를 나열하는 대신,<br>어디에 사는지를 그린다.",
            en: "176 entries, not listed.<br>Mapped.",
          },
        ],
        [
          "lede",
          {
            ko: `기본으로 로드되는 것은 팩 <strong>11벌 · DSL 룰 116개</strong>와
      <strong>네이티브 분석 60개</strong>(단일 트리 33 + 저장소 간 27)다. 원문 사이트는 이걸 176행짜리 표로 싣는다 —
      찾을 것이 있을 때는 정확하지만, <em>무엇을 잡는 도구인지</em>는 알려주지 않는다.
      이 페이지는 표가 아니라 지도다.`,
            en: `The default load is <strong>11 packs · 116 DSL rules</strong> plus
      <strong>60 native analyses</strong> (33 single-tree + 27 cross-repo). The source site prints all 176 as one
      table — exact when you
      already know what you're looking for, and silent about <em>what kind of tool this is</em>.
      This page is the map, not the table.`,
          },
        ],
        [
          "muted",
          {
            ko: `룰 id·팩 이름·억제 마커는 당신이 설정에 그대로 적는 문자열이다. 이 페이지의 것은 전부 원문 철자 그대로다.`,
            en: `Rule ids, pack names and suppress markers are strings you type into a config. Every one below keeps
      its original spelling.`,
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
            ["eyebrow", { ko: "무엇을 잡나", en: "What it catches" }],
            [
              "h2",
              {
                ko: "팩 열하나. 셋이 전체의 4분의 3이다.",
                en: "Eleven packs — three of them are three quarters of it.",
              },
            ],
          ],
        ],
        [
          "vs",
          [
            {
              k: "<code>security</code> · 49",
              v: {
                ko: `커밋된 시크릿, 인젝션, 약한 암호, 토큰과 CORS 설정 —
            <code>hardcoded-secret</code> · <code>sql-string-concat</code> · <code>weak-password-hash</code> ·
            <code>jwt-none-algorithm</code> · <code>cors-credentials-wildcard</code>.`,
                en: `Committed secrets, injection, weak crypto, token and CORS posture —
            <code>hardcoded-secret</code> · <code>sql-string-concat</code> · <code>weak-password-hash</code> ·
            <code>jwt-none-algorithm</code> · <code>cors-credentials-wildcard</code>.`,
              },
            },
            {
              k: "<code>db</code> · 21",
              v: {
                ko: `쓰기가 트랜잭션 밖에 있거나, 조건 없이 나가거나, 루프 안에서 반복되는 것 —
            <code>update-delete-no-where</code> · <code>multi-write-no-tx</code> · <code>connection-no-release</code>.`,
                en: `Writes outside a transaction, writes with no condition, writes repeated per iteration —
            <code>update-delete-no-where</code> · <code>multi-write-no-tx</code> · <code>connection-no-release</code>.`,
              },
            },
            {
              k: "<code>reliability</code> · 16",
              v: {
                ko: `기다릴 줄 모르는 호출과 정리되지 않는 자원 —
            <code>fetch-no-timeout</code> · <code>async-route-no-catch</code> · <code>sync-fs-in-handler</code> ·
            <code>interval-no-clear</code>.`,
                en: `Calls that never give up and resources nobody closes —
            <code>fetch-no-timeout</code> · <code>async-route-no-catch</code> · <code>sync-fs-in-handler</code> ·
            <code>interval-no-clear</code>.`,
              },
            },
            {
              k: "<code>sql</code> · 8",
              v: {
                ko: `쿼리를 세는 문제와 되돌릴 수 없는 문장 —
            <code>nplus1</code> · <code>delete-no-where</code> · <code>destructive-migration</code>.`,
                en: `Query-count problems and statements you can't undo —
            <code>nplus1</code> · <code>delete-no-where</code> · <code>destructive-migration</code>.`,
              },
            },
            {
              k: "<code>browser</code> · 8",
              v: {
                ko: `HTML 이 흘러드는 DOM 싱크 —
            <code>unsafe-html-sink</code> · <code>postmessage-wildcard</code> · <code>vue-v-html</code>.`,
                en: `DOM sinks that HTML flows into —
            <code>unsafe-html-sink</code> · <code>postmessage-wildcard</code> · <code>vue-v-html</code>.`,
              },
            },
            {
              k: "<code>redis</code> · 6",
              v: {
                ko: `운영에서 치면 안 되는 명령과, 풀리지 않는 락 —
            <code>flushall-in-code</code> · <code>keys-command-in-code</code> · <code>lock-no-ttl</code>.`,
                en: `Commands you must not run in production, and locks that never release —
            <code>flushall-in-code</code> · <code>keys-command-in-code</code> · <code>lock-no-ttl</code>.`,
              },
            },
            {
              k: "<code>egress</code> · 3",
              v: {
                ko: `밖으로 나가는 요청의 모양 —
            <code>http-url-literal</code> · <code>ws-no-auth</code> · <code>get-and-body</code>.`,
                en: `The shape of requests leaving your process —
            <code>http-url-literal</code> · <code>ws-no-auth</code> · <code>get-and-body</code>.`,
              },
            },
            {
              k: "<code>http</code> · 2",
              v: {
                ko: `보호받아야 할 경로에 인증의 흔적이 없는 것 —
            <code>protected-path-no-auth-evidence</code> · <code>dev-path-no-guard-hint</code>.`,
                en: `Paths that should be guarded, with no evidence of a guard —
            <code>protected-path-no-auth-evidence</code> · <code>dev-path-no-guard-hint</code>.`,
              },
            },
            {
              k: "<code>go</code> · <code>perf</code> · <code>react</code>",
              v: {
                ko: `셋 다 룰이 <strong>하나뿐</strong>이다 — <code>goroutine-in-loop</code> ·
            <code>api-in-loop</code> · <code>setstate-after-async-unguarded</code>. 팩은 분류이지 분량이 아니다.`,
                en: `One rule each — <code>goroutine-in-loop</code> · <code>api-in-loop</code> ·
            <code>setstate-after-async-unguarded</code>. A pack is a category, not a quota.`,
              },
            },
          ],
          { wide: true },
        ],

        [
          "note",
          {
            ko: `어떤 룰이 <strong>어느 언어에 닿는지는 팩이 아니라 룰마다</strong> 정해진다 — 룰 자신의
      <code>file_pattern</code> 하나가 답이고 팩 수준에는 그런 설정이 없다. 그래서 한 팩이 어떤 언어에는 빽빽하고
      다른 언어에는 텅 빌 수 있다. "이 언어에 룰이 몇 개냐"에는 답이 없다 — <em>이 경로</em>에 몇 개가 도는지를 물어야 한다.`,
            en: `Which languages a rule reaches is a <strong>per-rule</strong> fact, decided by that rule's own
      <code>file_pattern</code> — there is no pack-level equivalent. A pack can be dense for one language and
      empty for another, so "how many rules for language X" has no answer. Ask about a concrete path instead.`,
          },
          { inner: true },
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
            ["eyebrow", { ko: "왜 둘인가", en: "Why two kinds" }],
            [
              "h2",
              {
                ko: "룰은 파일 한 장을 본다. 나머지는 정규식으로 못 쓴다.",
                en: "A rule sees one file. The rest cannot honestly be a regex.",
              },
            ],
          ],
        ],
        [
          "lenses",
          [
            {
              h: { ko: "DSL 룰 · 116", en: "DSL rules · 116" },
              p: {
                ko: `파일 한 장 안에서 끝난다. 룰마다 matcher 모양을 정확히 하나 고르고, 두 번째 파일의 내용은 볼 수 없다.
          그 대신 JSON 이라 당신이 직접 쓸 수 있다.`,
                en: `Scoped to a single file — each rule declares exactly one matcher shape and cannot see a second
          file's content. In exchange, it's JSON, so you can write one.`,
              },
            },
            {
              h: { ko: "단일 트리 · 33", en: "Single-tree · 33" },
              p: {
                ko: `트리 하나 전체를 본다 — 의존 그래프, 죽은 코드, 스키마, 라우트. 이 중 다섯(<code>seams</code> ·
          <code>criticality</code> · <code>scores</code> · <code>health</code> · <code>recommendations</code>)은 발견이
          아니라 점수 계산이라 심각도 자체가 없다.`,
                en: `Whole-tree: the dependency graph, dead code, schema, routes. Five of them (<code>seams</code> ·
          <code>criticality</code> · <code>scores</code> · <code>health</code> · <code>recommendations</code>) are score
          computations, not findings, and carry no severity at all.`,
              },
            },
            {
              h: { ko: "크로스레이어 · 27", en: "Cross-layer · 27" },
              p: {
                ko: `저장소 <strong>여럿</strong>을 조인해야만 존재하는 발견이다. <code>zzop cross</code> 로만 돈다 —
          <code>cross-layer/method-mismatch</code> · <code>cross-layer/body-field-drift</code> ·
          <code>cross-layer/sensitive-response-field</code>.`,
                en: `Findings that only exist once <strong>several</strong> trees are joined — they run under
          <code>zzop cross</code> alone: <code>cross-layer/method-mismatch</code> ·
          <code>cross-layer/body-field-drift</code> · <code>cross-layer/sensitive-response-field</code>.`,
              },
            },
          ],
        ],
        [
          "group",
          null,
          [
            [
              "p",
              {
                ko: `경계는 취향이 아니라 <strong>표현 가능성</strong>이다. 네 가지는 줄 단위 정규식으로 정직하게 쓸 수 없다 —
        선언과 사용을 잇는 추적("선언됐는데 아무도 안 읽는다"), 파일을 가로지르는 조인(상수나 핸들러가 다른 파일에 있다),
        콜그래프 탐색("핸들러 X가, 혹은 X가 부르는 무언가가 Y를 한다"), 그리고 텍스트 동시출현이 아닌 진짜 AST·JSX 모양.
        그래서 그것들만 네이티브다.`,
                en: `The split is about <strong>expressibility</strong>, not taste. Four things have no honest
        regex-over-lines encoding: declaration-to-use tracking ("declared, never read"), a cross-file join (the
        constant or route handler lives in another file), call-graph traversal ("handler X, or something it calls
        transitively, does Y"), and real AST/JSX shape rather than text co-occurrence. Those, and only those, are native.`,
              },
            ],
            [
              "note",
              {
                ko: `끄는 법도 여기서 갈린다. DSL 룰은 인라인 주석 하나로 한 건만 조용히 시킬 수 있고, 네이티브 분석은
        <strong>설정으로만</strong> 끈다 — 예외 둘: <code>non-idempotent-write</code> · <code>unsafe-read-endpoint</code>
        는 손으로 쓴 <code>// idempotent-ok: &lt;reason&gt;</code> 를 존중하고(끝의 콜론 필수),
        <code>dead-candidates</code> · <code>unimported-export</code> 는 생성 파일 배너가 붙은 파일을 건너뛴다.`,
                en: `Turning them off differs too. A DSL rule can be silenced one finding at a time with an inline comment; a
        native analysis is <strong>disable-only</strong> — with two exceptions: <code>non-idempotent-write</code> and
        <code>unsafe-read-endpoint</code> honor a hand-written <code>// idempotent-ok: &lt;reason&gt;</code> (the
        trailing colon is required), and <code>dead-candidates</code> / <code>unimported-export</code> skip files
        carrying a generated-file banner.`,
              },
            ],
          ],
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "룰은 데이터다", en: "Rules are data" }],
        [
          "h2",
          {
            ko: "팩은 JSON 파일 하나다. 번들 룰과 당신 룰을 인터프리터는 구별하지 않는다.",
            en: "A pack is one JSON file — and the interpreter can't tell yours from the bundled ones.",
          },
        ],
        [
          "p",
          {
            ko: `룰은 컴파일된 코드가 아니라 <code>&lt;id&gt;.json</code> 한 장이다. 트리 안 <code>zzop/rules/</code> 에 넣으면
      설정 키 하나 없이 다음 실행부터 로드된다. 1급/3급 구분 같은 것은 <em>애초에 없다</em>.`,
            en: `A rule isn't compiled code, it's an <code>&lt;id&gt;.json</code> file. Drop one in a tree's
      <code>zzop/rules/</code> and the next run loads it, no config key needed. There is
      <em>no first-party / third-party distinction</em> at the interpreter level.`,
          },
        ],
        [
          "panel",
          {
            // 원본은 ko/en 두 탭이 글자까지 같다(경로 문자열). 언어 무관 문자열로 쓰면 탭이 하나만
            // 나와 panel__tab 개수가 4→3 이 되므로, 같은 값의 짝으로 둔다.
            tab: { ko: "zzop/rules/house-rules.json", en: "zzop/rules/house-rules.json" },
            lines: [
              `{`,
              `  <span class="hit">"id"</span>: "house-rules",`,
              `  "schema_version": 1,`,
              `  "rules": [`,
              `    {`,
              `      <span class="hit">"id"</span>: "hardcoded-debug-token",`,
              `      "severity": "warning",`,
              `      "message": "X-Debug-Token header set to a string literal — read it from env/config instead.",`,
              `      "matcher": {`,
              `        "type": "line-scan",`,
              `        "file_pattern": "(?i)\\\\.(ts|tsx)$",`,
              `        "require_file": "X-Debug-Token",`,
              `        "skip_comment_lines": true,`,
              `        "line_pattern": "[\\"']X-Debug-Token[\\"']\\\\s*:\\\\s*[\\"'][^\\"'\`]+[\\"']",`,
              `        "snippet_max": 160`,
              `      }`,
              `    }`,
              `  ]`,
              `}`,
              {
                comment: {
                  ko: `  두 id 가 곧 계약이다 → 발견은 house-rules/hardcoded-debug-token,
  마커는 // zzop-hardcoded-debug-token-ok`,
                  en: `  those two ids ARE the contract → findings say house-rules/hardcoded-debug-token,
  the marker is // zzop-hardcoded-debug-token-ok`,
                },
              },
            ],
          },
        ],
        [
          "muted",
          {
            ko: `고를 수 있는 matcher 는 여섯이다 — <code>line-scan</code>(한 줄의 모양) · <code>method-scan</code>(한 함수 안의
      동시출현) · <code>symbol-scan</code>(선언된 심볼) · <code>io-scan</code>(라우트·테이블 같은 IO 사실) ·
      <code>call-scan</code>(파서가 목격한 호출) · <code>literal-scan</code>(문자열의 이름·해시·엔트로피 — 값 자체는 절대 아니다).`,
            en: `Six matcher shapes to choose from — <code>line-scan</code> (one line's shape) · <code>method-scan</code>
      (co-occurrence inside one function) · <code>symbol-scan</code> (declared symbols) · <code>io-scan</code>
      (IO facts like routes and tables) · <code>call-scan</code> (calls the parser witnessed) ·
      <code>literal-scan</code> (a literal's binding name, hash and entropy — never the value itself).`,
          },
        ],
        [
          "vs",
          [
            {
              k: "<code>zzop/rules/</code>",
              v: {
                ko: `트리 안의 기본 자리. 아무것도 선언하지 않아도 주워 간다.`,
                en: `The default authored-pack location in a tree. Picked up with nothing declared.`,
              },
            },
            {
              k: "<code>packs.extraDirs</code>",
              v: {
                ko: `<code>zzop.config.jsonc</code> 에서 쓰는 철자. 디렉터리 하나 또는 배열.`,
                en: `The spelling in a <code>zzop.config.jsonc</code>. One directory, or an array.`,
              },
            },
            {
              k: "<code>packsDir</code>",
              v: {
                ko: `임베더의 요청 객체에서 쓰는 철자. 위와 <strong>같은 뜻이지만 서로 바꿔 쓸 수 없다.</strong>`,
                en: `The spelling on an embedder's request object — <strong>same intent, not interchangeable.</strong>`,
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `디렉터리는 각각 따로 로드된 뒤 <strong>팩 id 로 병합</strong>된다. 같은 id 가 두 곳에 있으면 뒤쪽 디렉터리의 팩이
      앞쪽을 <em>통째로 대체</em>한다 — 룰 단위 병합이 아니다. 엔진을 포크하지 않고 번들 팩을 통째로 덮는 길이 이것이다.
      번들에 안 들어간 완성 팩 넷은 <code>examples/packs/</code> 에 있고, 각각 계약 문서로도 제공된다.`,
            en: `Directories load independently, then <strong>merge by pack id</strong>: if the same id appears twice, the
      pack from the later directory replaces the earlier one <em>whole</em> — never a per-rule merge. That is how
      you override a bundled pack without forking the engine. Four finished packs deliberately kept out of the
      default set live in <code>examples/packs/</code>, each also served as a contract document.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "발견 하나", en: "One finding" }],
        [
          "h2",
          {
            ko: "발견은 문제만 들고 오지 않는다. 끄는 법을 같이 들고 온다.",
            en: "A finding doesn't just carry the problem. It carries its own off switch.",
          },
        ],
        [
          "panel",
          {
            tab: {
              ko: "발견 하나가 실제로 담는 것",
              en: "what a single finding actually carries",
            },
            lines: [
              `db/float-money-compare   info   src/billing/invoice.ts:212`,
              ``,
              `  A money-named identifier (\`price\`/\`amount\`/\`balance\`/\`fee\`/\`cost\`) compared`,
              `  with \`==\`/\`===\`/\`!=\`/\`!==\` against a float literal (e.g. \`price === 19.99\`)`,
              `  — floating-point rounding error makes strict equality on monetary values`,
              `  unreliable. Represent money as integer minor units (cents) or a decimal library.`,
              {
                comment: {
                  ko: `     여기까지가 룰 저자가 쓴 문장`,
                  en: `     the rule author wrote this much`,
                },
              },
              ``,
              `  <span class="hit">Suppress a vetted case with \`// zzop-float-money-compare-ok\`.</span>`,
              `  <span class="hit">Disable via config \`rules: { "db/float-money-compare": "off" }\` (embedders: \`disabledRules\`)</span>`,
              {
                comment: {
                  ko: `     이 두 줄은 엔진이 붙인다`,
                  en: `     the engine appends these two, always`,
                },
              },
            ],
          },
        ],
        [
          "p",
          {
            ko: `원인과 고치는 법, 이 한 건만 조용히 시키는 주석, 그리고 이 룰을 실행 단위로 끄는 설정 키 — 셋이 한 덩어리로 온다.
      "이건 왜 뜨지"와 "어떻게 끄지"를 문서에서 찾을 일이 없다.`,
            en: `The cause and the fix, the comment that silences this one case, and the config key that turns the rule off
      for the whole run — one package. You never go to the docs to ask why it fired or how to stop it.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "마커는 파생된다", en: "Derived, not stored" },
              v: {
                ko: `룰 id 에서 <code>zzop-&lt;rule id&gt;-ok</code> 로 계산된다 — 어디에도 저장되지 않으니 낡을 수가 없다.
            <strong>팩 접두사는 붙지 않는다</strong>: <code>security/hardcoded-secret</code> →
            <code>// zzop-hardcoded-secret-ok</code>.`,
                en: `Computed from the rule id as <code>zzop-&lt;rule id&gt;-ok</code> — stored nowhere, so it can
            never drift. <strong>The pack prefix is stripped</strong>: <code>security/hardcoded-secret</code> →
            <code>// zzop-hardcoded-secret-ok</code>.`,
              },
            },
            {
              k: { ko: "어디에 두나", en: "Where it goes" },
              v: {
                ko: `발견된 줄 자신, 또는 <strong>바로 윗줄 한 줄</strong>. 그보다 위는 안 먹는다 — 창을 넓히면
            한 호출을 겨냥한 마커가 아래쪽의 검토도 안 한 발견들을 조용히 삼킨다.`,
                en: `The finding's own line, or the <strong>single line directly above it</strong> — nowhere
            further back. A wider window lets a marker aimed at one call silently swallow unvetted findings below it.`,
              },
            },
            {
              k: { ko: "마커가 없는 것", en: "No marker at all" },
              v: {
                ko: `<code>symbol-scan</code> 발견은 주석을 걸 소스 줄 개념이 없어서 인라인 마커를 갖지 않는다.
            그런 것도 <code>rules: { "&lt;id&gt;": "off" }</code> 로는 언제나 끌 수 있다.`,
                en: `<code>symbol-scan</code> findings have no source-line concept to anchor a comment against,
            so they carry none. They are still always turnable off with
            <code>rules: { "&lt;id&gt;": "off" }</code>.`,
              },
            },
            {
              k: { ko: "직접 쓰지 마라", en: "Don't write it yourself" },
              v: {
                ko: `당신 룰의 <code>message</code> 에 마커나 끄는 법을 적으면 <strong>두 번 찍힌다</strong>. 게다가
            matcher 종류를 바꾸는 순간, 손으로 쓴 문장은 엔진이 더는 인정하지 않는 주석 기호를 가리키게 된다.`,
                en: `Naming the marker or the disable hint in your own rule's <code>message</code>
            <strong>renders it twice</strong> — and the hand-written sentence goes stale the moment the matcher kind
            changes, because it names comment leaders the engine no longer honours.`,
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `176행 전부 — 룰마다의 심각도·matcher·정확히 무엇을 잡는지, 그리고 네이티브 60개의 표 — 는
      <a href="https://eezz4.github.io/zzop/rules.html" target="_blank" rel="noreferrer">원문 카탈로그</a>에 있다.
      그 페이지는 레포의 <code>docs/rules/catalog.md</code> 에서 생성되고, 거기 적힌 모든 id 가 엔진이 실제로 로드하는 것과
      같은지는 Rust 메타테스트가 기계로 확인한다 — 카탈로그는 코드와 조용히 어긋날 수 없다.`,
            en: `All 176 rows — every rule's severity, matcher and exact subject, plus the 60-row native table — live in the
      <a href="https://eezz4.github.io/zzop/rules.html" target="_blank" rel="noreferrer">source catalog</a>. That page
      is generated from <code>docs/rules/catalog.md</code> in the repo, and a Rust meta-test machine-checks that every
      id listed there is one the engine actually loads — the catalog cannot silently drift from the code.`,
          },
        ],
      ],
    },
  ],
};
