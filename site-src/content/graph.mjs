// 그래프 — Graph
// 문장은 여기 한 번만 산다. HTML 은 scripts/site/render.mjs 가 만든다.

export default {
  bands: [
    {
      header: true,
      blocks: [
        ["eyebrow", { ko: "그래프", en: "Graph" }],
        [
          "h1",
          {
            ko: "zzop 은 표 두 개를 낸다.<br>그림은 다른 프로그램이 그린다.",
            en: "zzop emits two tables.<br>Something else draws the picture.",
          },
        ],
        [
          "lede",
          {
            ko: `<code>zzop graph</code> 는 분석 결과를 <strong>표준 그래프 포맷</strong>으로 직렬화해 stdout 으로 흘려보내고 끝난다.
      픽셀을 그리는 코드는 이 저장소에 <em>한 줄도 없다</em>. 뷰어는 당신이 고른다.`,
            en: `<code>zzop graph</code> serializes the analysis into a <strong>standard graph format</strong>, writes it to stdout,
      and stops there. <em>Not one line of pixel-drawing code lives in this repository.</em> You pick the viewer.`,
          },
        ],
        [
          "muted",
          {
            ko: `그 표를 읽은 그림 — 이 저장소 자신의 import 그래프 — 은 <strong>이 페이지 맨 아래</strong>에 있다.
      좌표와 통계는 이 페이지가 소유하지 않는다. 빌드할 때마다
      <a href="https://eezz4.github.io/zzop/graph.html" target="_blank" rel="noreferrer">원문 그래프 페이지</a>에서 그대로 가져온다.`,
            en: `The picture those tables produce — this repository's own import graph — is <strong>at the foot of this page</strong>.
      Its coordinates and counts are not owned here: every build pulls them from the
      <a href="https://eezz4.github.io/zzop/graph.html" target="_blank" rel="noreferrer">source graph page</a> as they stand.`,
          },
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "출력", en: "Output" }],
        ["h2", { ko: "커맨드 둘, 표 둘.", en: "Two commands, two tables." }],
        [
          "p",
          {
            ko: `뷰어가 반드시 필요로 하는 것은 <strong>링크 표</strong>이고 노드 표는 스타일용 선택지다.
      표는 둘인데 stdout 은 하나이고 zzop 은 파일을 쓰지 않는다 — 그래서 포맷 이름이 곧 선택기다.`,
            en: `A viewer strictly requires the <strong>links table</strong>; the nodes table is the optional styling half.
      Two tables, one stdout, and zzop writes no files — so the format name is the selector.`,
          },
        ],
        [
          "run",
          [
            {
              cmd: "zzop graph --domain dep --format cosmograph-links &gt; links.ndjson",
              what: { ko: "엣지 표 · 필수", en: "the edges · required" },
            },
            {
              cmd: "zzop graph --domain dep --format cosmograph-nodes &gt; nodes.ndjson",
              what: { ko: "점 표 · 스타일 축", en: "the points · styling axes" },
            },
          ],
        ],
        [
          "panel",
          {
            tab: { ko: "두 표에서 한 줄씩", en: "One row from each table" },
            lines: [
              {
                comment: {
                  ko: "links.ndjson — 한 줄이 import 하나",
                  en: "links.ndjson — one line is one import",
                },
              },
              `{"endpointsInCycle":false,<span class="hit">"source"</span>:"src/app.ts",<span class="hit">"target"</span>:"src/db.ts"}`,
              ``,
              {
                comment: {
                  ko: "nodes.ndjson — 한 줄이 파일 하나",
                  en: "nodes.ndjson — one line is one file",
                },
              },
              `{"degree":7,"fanIn":6,"fanOut":1,"folder":"src","id":"src/db.ts","inCycle":false,"label":"db.ts","loc":214,"path":"src/db.ts","source":"web"}`,
              {
                comment: {
                  ko: "   git 수집이 돌았으면 changeCount · churn · authorCount · lastModified 가 더 붙는다",
                  en: "   a run that collected git adds changeCount · churn · authorCount · lastModified",
                },
              },
            ],
          },
        ],
        // 두 표의 열 전체. 위 샘플은 **어떤 한 번의 실행**이 낸 줄이라 "늘 있는 열"과 "그 실행이
        // 재야만 붙는 열"을 구분해 주지 못하는데, 이 레인에서 그 구분이 곧 스키마다(바로 아래
        // 밴드의 "안 잰 축"이 왜 그런지를 말한다).
        //
        // 아래 런은 일부러 **언어 중립 문자열**이다 — 열 목록을 에디션마다 한 번씩 쓰면 한쪽만
        // 고쳐지는 날이 오고, 그게 이 구조가 존재하는 이유 그 자체다. 그리고 crates/summary 의
        // 테스트 하나가 생성된 영문 페이지에서 이 `/` 로 이은 런을 그대로 읽어 에미터가 실제로
        // 쓰는 키 집합과 **집합으로** 비교한다: 여기서 열 하나를 빼거나 더하면(혹은 에미터만
        // 고치면) 그 테스트가 빨개진다. 2026-07-29 에 옛 usage.html 이 노드 목록을 아홉에서
        // 일곱으로 흘려 `source` 와 `path` 를 조용히 떨어뜨린 사고가 그 테스트를 낳았다.
        [
          "p",
          {
            ko: `위 샘플은 <em>어떤 한 번의 실행</em>이 낸 줄이라, 어느 열이 늘 있고 어느 열이 그 실행이 무엇을 쟀느냐에 달렸는지는 거기서 읽히지 않는다. 그 갈래가 곧 스키마다.`,
            en: `That sample is one row from <em>one particular run</em>, so it cannot tell you which columns are always
      there and which ones the run had to measure first. That split is the schema.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "링크 표", en: "Links, every row" },
              v: "<code>source</code>/<code>target</code>/<code>endpointsInCycle</code>",
            },
            {
              k: { ko: "노드 표 · 모든 행", en: "Nodes, every row" },
              v: "<code>id</code>/<code>source</code>/<code>path</code>/<code>label</code>/<code>folder</code>/<code>fanIn</code>/<code>fanOut</code>/<code>degree</code>/<code>inCycle</code>",
            },
            {
              k: { ko: "노드 · loc 을 읽었으면", en: "Nodes, if loc was read" },
              v: "<code>loc</code>",
            },
            {
              k: { ko: "노드 · git 을 걷었으면", en: "Nodes, if git was collected" },
              v: "<code>authorCount</code>/<code>changeCount</code>/<code>churn</code>/<code>lastModified</code>",
            },
          ],
        ],
        [
          "p",
          {
            ko: `<code>source</code> · <code>target</code> 은 뷰어의 매핑 단계가 이미 짐작하는 철자라 흔한 경우엔 매핑할 것이 없다.
      화살표 방향은 <strong>import 하는 쪽 → import 되는 쪽</strong>이다.`,
            en: `<code>source</code> and <code>target</code> are spelled the way a viewer's mapping step already guesses,
      so the common case needs no mapping at all. The direction is <strong>importer → imported</strong>.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "안 잰 축", en: "An unmeasured axis" },
              v: {
                ko: `이번 실행이 재지 못한 값은 <strong>열 자체가 없다</strong>. <code>churn: 0</code> 은 “한 번도 안 바뀌었다”와 “아무도 안 봤다”를 같은 바이트로 쓰기 때문이다.`,
                en: `A value this run didn't measure is an <strong>absent key</strong>, never a zero: <code>churn: 0</code> would spell "never changed" in the same bytes as "nobody looked".`,
              },
            },
            {
              k: "<code>endpointsInCycle</code>",
              v: {
                ko: `“이 엣지가 순환 위에 있다”가 아니라 <strong>“양 끝이 둘 다 순환 멤버다”</strong>. 검사한 것만 말하는 이름이다.`,
                en: `Not "this edge lies on a cycle" but <strong>"both ends are cycle members"</strong> — the name states only what was checked.`,
              },
            },
            {
              k: { ko: "통계 줄", en: "The census" },
              v: {
                ko: `몇 개 중 몇 개를 냈는지는 <strong>stderr</strong> 로 간다. stdout 이 파싱 가능한 표로 남아야 <code>&gt;</code> 로 그대로 받을 수 있다.`,
                en: `How many of how many got emitted rides <strong>stderr</strong>. stdout stays a parseable table, so <code>&gt;</code> catches it whole.`,
              },
            },
          ],
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "결정", en: "The decision" }],
        [
          "h2",
          { ko: "그리는 쪽은 갈아끼울 수 있다.", en: "The drawing half is replaceable." },
        ],
        [
          "p",
          {
            ko: `렌더러를 우리가 가지면 좌표계와 라이브러리와 뷰어를 같이 갖게 된다 — 그리고 그건 분석 엔진이 할 일이 아니다.
      표는 그 반대다. 같은 두 파일이 Cosmograph 든 Gephi 든 아무 force-graph 라이브러리든 변환 없이 들어간다.`,
            en: `Owning a renderer means owning a coordinate system, a library and a viewer — none of which is an analysis
      engine's job. A table is the opposite: the same two files load into Cosmograph, Gephi or any force-graph
      library without conversion.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "캡이 없다", en: "Uncapped" },
              v: {
                ko: `그려지는 그림은 캡이 필요하다 — mermaid 는 <code>dep</code> 를 기본 40 노드에서 자른다. 수천 개를 다 그리면 검은 사각형이 되고, 그건 아무것도 안 그린 것보다 나쁘다(정보처럼 보이니까). 이 레인은 줌이 그 일을 하므로 전부 낸다.`,
                en: `A drawn picture needs a cap — mermaid stops <code>dep</code> at 40 nodes by default. Drawing thousands produces a black square, which is worse than drawing nothing because it looks like information. This lane is uncapped instead: zoom does that job.`,
              },
            },
            {
              k: { ko: "거절한다", en: "It refuses" },
              v: {
                ko: `<code>--top</code> 이나 <code>--fold</code> 를 이 포맷에 주면 <strong>오류로 멈춘다</strong>. 받아 놓고 무시하는 플래그를 두지 않는다. <code>--format cosmograph-*</code> 는 <code>--domain dep</code> 에서만 쓴다.`,
                en: `Pass <code>--top</code> or <code>--fold</code> to this format and it <strong>exits with an error</strong> — no flag is accepted and then ignored. <code>--format cosmograph-*</code> requires <code>--domain dep</code>.`,
              },
            },
            {
              k: { ko: "좁히기는 하나", en: "One narrowing knob" },
              v: {
                ko: `<code>--scope &lt;prefix&gt;</code> 뿐이다. 한쪽 끝이 밖으로 밀려난 엣지는 같이 버린다 — 표에 없는 노드를 가리키는 행을 남기지 않으려고.`,
                en: `Only <code>--scope &lt;prefix&gt;</code>. An edge with one end outside the filter is dropped too, so no row points at a node the table doesn't contain.`,
              },
            },
          ],
        ],
      ],
    },

    {
      tint: true,
      blocks: [
        ["eyebrow", { ko: "고를 수 있는 것", en: "What you can ask for" }],
        [
          "h2",
          { ko: "그림은 다섯 종류, 직렬화는 셋.", en: "Five pictures, three serializations." },
        ],
        [
          "vs",
          [
            {
              k: "<code>--domain join</code>",
              v: {
                ko: "기본값. 교차 계층 조인 — 노드가 io 키다. 기본 캡 <strong>25 — 버킷별</strong> 그리는 관계 수라, 여섯 버킷을 다 채우면 문서 전체는 25보다 크다. 큰 <code>edges</code> 목록이 다른 버킷을 그림 밖으로 밀어내지 못하게 하려는 것이다.",
                en: "The default. The cross-layer join — nodes are io keys. Default cap <strong>25 — drawn relations per bucket</strong>, so a document with all six buckets populated carries more than 25 in total: a big <code>edges</code> list must not push a whole other bucket out of the picture.",
              },
            },
            {
              k: "<code>--domain dep</code>",
              v: {
                ko: "파일 import 그래프. 노드가 파일이고 순환은 다르게 그려진다. 위 두 표가 나오는 곳이 여기다. 기본 캡 <strong>40 — 노드에</strong> 걸린다(엣지는 살아남은 노드를 따라간다). 다섯 중 가장 크다.",
                en: "The file import graph — nodes are files, cycles drawn distinctly. This is the domain the two tables above come from. Default cap <strong>40 — on nodes</strong> (edges follow the surviving nodes), the largest of the five.",
              },
            },
            {
              k: "<code>--domain risk</code>",
              v: {
                ko: "터지면 넓게 번지는 허브와 추출이 끊긴 이음매. 기본 캡 <strong>12</strong>로 다섯 중 가장 작고, <strong>종류별</strong>로 건다 — 허브 목록이 길다고 이음매가 그림 밖으로 밀려나지 않게. 두 목록은 엔진이 이미 순위를 매겨 짧게 낸 것이라, 이 수는 잘라내기 정책이 아니라 가독성 한계다.",
                en: "Blast-radius hubs and extraction seams. Default cap <strong>12</strong>, the smallest of the five, and applied <strong>per kind</strong> so a long hub list cannot push every seam out of the picture. Both lists arrive engine-ranked and already short: this number is a readability bound, not a truncation policy.",
              },
            },
            {
              k: "<code>--domain posture</code>",
              v: {
                ko: "상태를 바꾸는 공격면과 그 가드 상태. 기본 캡은 <strong>트리별 20</strong>개 라우트 — 이 그림은 분류하라고 있는 것이고 라우트 백 개는 벽이다.",
                en: "The mutating attack surface and its guard status. The default cap is <strong>20 routes per tree</strong>: this picture is for triage, and a hundred routes is a wall.",
              },
            },
            {
              k: "<code>--domain cochange</code>",
              v: {
                ko: `git 동시 변경. <code>dep</code> 와 같은 노드 위의 <em>다른</em> 관계라 겹치지 않고 따로 선다 — import 는 소스에서 읽고, 동시 변경은 이력의 표본이다. 기본 캡 <strong>30</strong> 으로 <code>dep</code> 보다 낮다: 여기 엣지는 독자가 비교해야 하는 가중치를 달고 있어서, 마흔 개면 이미 그림이 아니라 목록으로 읽힌다.`,
                en: `Git co-change: a <em>different</em> relation over the same nodes as <code>dep</code>, so it stands apart rather than blending — an import is read from source, a co-change is a sample of history. Default cap <strong>30</strong>, lower than <code>dep</code>'s: a co-change edge carries a weight the reader has to compare, and forty weighted edges is already past the point where a flowchart reads as a picture rather than a list.`,
              },
            },
          ],
          { wide: true },
        ],
        [
          "note",
          {
            ko: `<code>--format</code> 은 <code>mermaid</code>(기본) · <code>cosmograph-nodes</code> · <code>cosmograph-links</code> 셋이다.
      mermaid 는 다섯 도메인 전부를 플로차트 텍스트로 내고, cosmograph 표는 <code>dep</code> 하나에만 있다.
      이 레인은 CLI 전용이다 — MCP 도구 쌍이 없다.
      <code>--top</code> 의 기본값이 도메인마다 다른 것은 밀도가 다르기 때문이다 — 조인은 관계가 수십인데 import 그래프는 수천이다.
      다섯 값은 전부 <code>zzop graph --help</code> 가 직접 찍는다(이 페이지가 아니라 그쪽이 정본이다).
      그리고 잘린 만큼은 문서 안에 공시된다 — 센서스 한 줄과 눈에 보이는 노트 노드로.`,
            en: `<code>--format</code> takes <code>mermaid</code> (the default), <code>cosmograph-nodes</code> or
      <code>cosmograph-links</code>. Mermaid serializes all five domains as flowchart text; the cosmograph tables
      exist for <code>dep</code> alone. This lane is CLI-only — it has no MCP tool twin.
      <code>--top</code> defaults differ per domain because their densities do: a join has tens of relations where an
      import graph has thousands. All five are printed by <code>zzop graph --help</code> itself, which owns them —
      not this page. Whatever a cap removes is disclosed inside the document, as a census line and a visible note node.`,
          },
        ],
      ],
    },

    {
      blocks: [
        ["eyebrow", { ko: "읽는 법", en: "Reading it" }],
        ["h2", { ko: "배치는 zzop 이 정한 것이 아니다.", en: "The layout is not zzop's." }],
        [
          "p",
          {
            ko: `아래는 원문 페이지의 뷰어가 그 표를 어떻게 배치했는지다 — 그 규칙은 zzop 의 출력에 들어 있지 않다.
      같은 두 표로 다른 배치를 만들 수 있다는 것, 그게 이 페이지 전체의 논지다.`,
            en: `What follows is how the source page's viewer chose to arrange those tables — rules that live nowhere in
      zzop's output. That the same two tables support a different arrangement is this page's whole argument.`,
          },
        ],
        [
          "vs",
          [
            {
              k: { ko: "점과 선", en: "Dots and lines" },
              v: {
                ko: "점 하나가 파일 하나, 선 하나가 import 하나. 기본 크기는 차수(degree)다.",
                en: "One dot per file, one line per import; size is degree by default.",
              },
            },
            {
              k: { ko: "가로축", en: "Left to right" },
              v: {
                ko: `의존 방향이다. <strong>맨 왼쪽은 이 저장소의 무엇도 그것을 import 하지 않는 파일</strong>이고, 오른쪽으로 갈수록 <strong>결국 모두가 기대는 것</strong>이 나온다. 순환에는 깊이가 없어서 — 순환 멤버 중 누구도 다른 멤버보다 위가 아니다 — 한 열에 통째로 뭉친다.`,
                en: `That axis is the dependency direction: <strong>the far left is what nothing in the repository imports</strong>, and the far right is <strong>what everything ends up leaning on</strong>. A cycle has no such depth — no member sits above another — so it collapses into a single column.`,
              },
            },
            {
              k: { ko: "오른쪽 밖의 블록", en: "The block past the edge" },
              v: {
                ko: `양방향 모두 0인 파일들이라 축 위에 자리가 없다. 대부분 결함이 아니다 — <strong>러너가 부르는 진입점</strong>(테스트 타깃, <code>bin</code> 루트, 스크립트, <code>&lt;script&gt;</code> 가 불러오는 파일)은 정의상 아무도 import 하지 않는다.`,
                en: `Files with no imports in either direction, so they have no place on the axis. Most are not defects — <strong>a runner calls them</strong> (test targets, <code>bin</code> roots, scripts, files a <code>&lt;script&gt;</code> tag loads), so nothing imports them by definition.`,
              },
            },
            {
              k: { ko: "두 배율", en: "Two grains" },
              v: {
                ko: `축소하면 최상위 영역마다 버블 하나, 확대하면 그 안의 파일들로 풀린다. 버블은 자기가 요약하는 파일들의 위치에서 나온 것이라 두 배율이 서로 어긋나지 않는다.`,
                en: `Zoomed out, one bubble per top-level area; zoom in and it resolves into the files inside. The bubbles are placed from the very file positions they summarize, so the two grains cannot contradict each other.`,
              },
            },
          ],
        ],
        [
          "note",
          {
            ko: `그 그림과 거기 적힌 수치는 <strong>스냅숏</strong>이다 — 페이지를 다시 생성한 시점의 트리를 서술하고, CI 에 물려 있는 것은 없다.
      위의 두 커맨드를 지금 체크아웃에 다시 돌리는 것이 재계수다. 데이터를 여기 옮겨 담지 않은 이유도 그것이다.`,
            en: `That drawing and every count on it are a <strong>snapshot</strong>: they describe the tree the page was last
      regenerated against, and nothing wires it to CI. Re-running the two commands above on your current checkout is
      the recount — which is also why none of that data is copied onto this page.`,
          },
        ],
      ],
    },

    // 그림 자체. 위 밴드가 읽는 법을 다 말한 뒤에 온다 — 설명을 먼저, 그림을 나중에.
    // 여기 블록은 **자리만** 낸다: 좌표·통계는 이 파일에 없고, 조립기가 빌드 때
    // site/graph.html 에서 잘라 온다. 그래서 원본이 다시 생성되면 이 페이지도 같이 따라간다.
    {
      tint: true,
      wide: true,
      blocks: [
        ["eyebrow", { ko: "그림", en: "The picture" }],
        [
          "h2",
          {
            ko: "같은 두 표를, 뷰어 하나가 읽은 것.",
            en: "Those same two tables, read by one viewer.",
          },
        ],
        [
          "p",
          {
            ko: `끌면 움직이고 굴리면 확대된다. 점 위에 올리면 그 파일의 경로와 <code>fanIn</code>·<code>fanOut</code>·차수가 왼쪽 패널에 나온다.
      색과 크기의 축은 그 패널 위쪽에서 바꾼다 — <strong>바뀌는 것은 보이는 방식뿐이고, 표는 그대로다</strong>.`,
            en: `Drag to pan, scroll to zoom. Hover a dot and the left panel names the file's path and its
      <code>fanIn</code> · <code>fanOut</code> · degree. The colour and size axes switch at the top of that panel —
      <strong>what changes is the rendering, never the table</strong>.`,
          },
        ],
        [
          "graph",
          {
            canvas: {
              ko: "zzop 저장소의 import 의존성 그래프 — 점이 파일, 선이 import 하나.",
              en: "Dependency graph of the zzop repository — one dot per file, one line per import.",
            },
            colourBy: { ko: "색의 축", en: "Colour by" },
            sizeBy: { ko: "크기의 축", en: "Size by" },
            byArea: { ko: "영역", en: "Area" },
            byDegree: { ko: "차수", en: "Degree" },
            byFanIn: { ko: "들어옴", en: "Fan-in" },
            byFanOut: { ko: "나감", en: "Fan-out" },
            find: { ko: "찾기", en: "Find" },
            findHint: { ko: "경로 조각…", en: "path fragment…" },
            zoomIn: { ko: "확대", en: "Zoom in" },
            zoomOut: { ko: "축소", en: "Zoom out" },
            zoomReset: { ko: "처음 배율로", en: "Reset the view" },
            caption: {
              ko: `위 줄은 그 커맨드가 <strong>stderr</strong> 로 찍는 통계다 — stdout 은 파싱 가능한 표로 남는다.
      노드와 엣지가 몇 개인지는 이 산문 어디에도 박혀 있지 않다 — <strong>뷰어가 실린 표를 그 자리에서 센다</strong>.
      숫자의 주인이 하나여야 그래프를 다시 재도 이 페이지가 낡지 않는다.
      배치는 미리 계산되어 데이터에 실려 있어, 이 그림은 누가 언제 열어도 같다.`,
              en: `The line above is what the command prints on <strong>stderr</strong> — stdout stays a parseable table.
      No node or edge count is hardcoded anywhere in this prose: <strong>the viewer counts the loaded table on the spot</strong>,
      so the numbers have one owner and re-measuring the graph can never leave this page stale.
      The layout is precomputed and rides in the data, so this picture is the same on every visit.`,
            },
          },
        ],
      ],
    },
  ],
};
