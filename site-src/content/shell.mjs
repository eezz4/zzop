// 셸 — 페이지가 아니라 **페이지를 감싸는 것**의 문장들.
// 제목·<head> 설명·내비 라벨·푸터·언어 링크·그래프 뷰어 UI.
//
// 왜 여기 있나: 이것들은 원래 조립기(`_assemble2.mjs`) 안에 한/영 리터럴로 박혀 있었다.
// 조립기가 `scripts/gen-site.mjs` 로 들어오면서 그 자리가 사라졌다 — CI 의
// check-english-source.sh 는 레포 전체에서 비라틴 문자를 막고, 한국어가 살 수 있는 경로는
// `site-src/content/` 하나로 좁혀졌기 때문이다. 문장이 사는 유일한 곳이 content/ 라는
// 규약이 이 파일을 만든 이유이고, 이 파일이 있어서 예외 경로가 하나로 남는다.
//
// 다른 content/*.mjs 와 달리 `bands` 가 없다 — 렌더러를 타지 않고 조립기가 직접 읽는다.

export default {
  // 브라우저 탭·검색 결과의 제목.
  title: {
    ko: "zzop — 에이전트가 못 읽는 코드베이스를 읽는다",
    en: "zzop — read the codebase your agent can't",
  },

  // <meta name="description">. 태그도 따옴표도 못 들어간다(속성값이다).
  // 영어 쪽은 개편 전 site/index.html 에 실려 있던 문장 그대로다 — 이미 공개된 문장이고
  // 레포의 프로즈 가드들이 이미 판정한 문장이라, 새로 쓰면 그 판정을 처음부터 다시 받는다.
  description: {
    ko: "AI 코딩 에이전트가 컨텍스트에 못 담는 코드베이스를 zzop 이 읽고, 매번 같은 방식으로 답한다 — 어떤 프론트엔드 호출이 어떤 백엔드 라우트에 닿는지, 무엇이 위험하고, 무엇이 죽어 있고, 이번 실행이 무엇을 못 봤는지. 결정적이고, 로컬에서 돌고, Rust 로 짰다.",
    en: "zzop reads the codebase your AI agent can't fit in context and answers the same way every time — which frontend calls reach which backend routes, what's risky, what's dead, and what the run could not see. Deterministic, local, written in Rust.",
  },

  // 탭 목록. `id` 는 섹션 id 이자 딥링크 hash 이고, `src` 는 content/<src>.mjs 다 —
  // 둘 다 언어 무관 식별자라 번역하지 않는다. 순서가 곧 내비 순서다.
  // 구페이지 리다이렉트(site/architecture.html·site/usage.html)가 `#p-arch`·`#p-usage` 를
  // 가리키므로 그 둘의 id 는 함부로 바꾸면 안 된다.
  pages: [
    { id: "p-index", src: "index", label: { ko: "개요", en: "Overview" } },
    { id: "p-arch", src: "arch", label: { ko: "구조", en: "How it works" } },
    { id: "p-usage", src: "usage", label: { ko: "사용법", en: "Usage" } },
    { id: "p-contract", src: "contract", label: { ko: "계약", en: "Contract" } },
    { id: "p-rules", src: "rules", label: { ko: "룰", en: "Rules" } },
    { id: "p-graph", src: "graph", label: { ko: "그래프", en: "Graph" } },
  ],

  // Nav entries that are NOT in-page tabs but LINKS to a standalone page in site/.
  // `pages` above are SPA sections toggled by JS (each `#id` matches a `.page`);
  // these have an `href` instead and the assembler renders them WITHOUT the
  // `.nav__link` class, so the section-toggle script leaves them alone and the
  // browser follows the link. This is how the X showcase (site/x-showcase.html),
  // a separate page rather than a tab, is reachable from the top bar.
  // Rendered on BOTH editions so the menu LAYOUT is identical in English and
  // Korean (the user asked for parity: the item was missing from the Korean bar).
  // The target is an English-only page with no site/ko/ copy, so the label stays
  // in ENGLISH on purpose even in the Korean nav: an English label makes it plain
  // the destination is English, rather than a Korean label ("실전") that silently
  // lands on an English page. The Korean edition resolves the href to
  // ../x-showcase.html via fixRootLinks (site/ko/ is one directory down).
  extras: [
    { href: "x-showcase.html", label: "In the field" },
  ],

  // 언어 전환 링크의 aria-label. 링크에 보이는 글자("EN · KO")는 언어 무관이라 조립기가 낸다.
  langLink: {
    ko: "영어판으로 보기",
    en: "Read this in Korean",
  },

  // 푸터의 레퍼런스 포인터. **영어판 전용이다** — 가리키는 페이지(rules · reference · graph ·
  // x-showcase)는 전부 영어 전용 단독 페이지이고 site/ko/ 사본이 없다. 한국어판에서 이걸 링크하면
  // 한국어 메뉴가 영어 페이지로 튕긴다(2026-08-16 사용자가 이 증상을 지적). 한국어 독자가 영어
  // 레퍼런스로 가고 싶으면 상단 EN·KO 토글로 영어판에 넘어가면 되고, 거기 nav 가 그 페이지들을
  // 준다 — 그래서 한국어판 푸터는 이 span 을 렌더하지 않는다(scripts/gen-site.mjs).
  // 한국어판이 자체적으로 갖는 룰·그래프는 영어 단독 페이지가 아니라 SPA 탭(#p-rules · #p-graph)이다.
  footEnOnly: `<span class="muted" style="flex-basis:100%;max-width:38rem">
    The full rule catalog, the field-by-field JSON reference, the live import graph, and zzop run
    over everything X open-sourced are reference material and stay on their own pages:
    <a href="rules.html">rules</a> · <a href="reference.html">reference</a> · <a href="graph.html">graph</a> · <a href="x-showcase.html">in the field</a>.
  </span>`,

  // 그래프 뷰어가 **화면에 내는** 영어 문장. 원본 site/graph.html 안에서는 스크립트 리터럴로
  // 박혀 있어 번역할 자리가 없었다 — 조립기가 뷰어를 잘라 올 때 모드별로 갈아끼운다.
  // 캔버스가 그리는 글자(파일 이름·도메인 이름)는 데이터라 번역 대상이 아니고,
  // 여기 있는 여섯이 뷰어 UI 문자열의 전부다.
  // `{files}`·`{edges}` 는 뷰어가 실제 개수로 치환하는 자리표시자다 — 지우면 숫자가 사라진다.
  viewer: {
    census: {
      ko: "파일 {files}개 중 {files}개, import 엣지 {edges}개 중 {edges}개, 순환 발견 0건. 캡 없음 — 이 포맷에 --top 은 적용되지 않는다.",
      en: "{files} of {files} files, {edges} of {edges} import edges, 0 circular finding(s). UNCAPPED, --top does not apply to this format.",
    },
    empty: {
      ko: "축소하면 최상위 영역마다 버블 하나, 확대하면 그 안의 파일로 풀린다. 왼쪽은 아무도 import 하지 않는 것, 오른쪽은 결국 모두가 기대는 것. 점 위에 올리면 경로와 차수가 나온다.",
      en: "Zoomed out you see one bubble per top-level area; zoom in and it resolves into the files inside. Left is what nothing imports, right is what everything leans on. Hover a file for its path and degree.",
    },
    fanIn: { ko: "import 됨", en: "imported by" },
    fanOut: { ko: "import 함", en: "imports" },
    degree: { ko: "차수", en: "degree" },
    cyc: { ko: "순환 안", en: "in cycle" },
  },

  // 한국어판 폰트 스택에만 덧붙는 "Malgun Gothic" 의 한국어 이름.
  // 스타일시트(site-src/site-v2.css)는 영어 전용 경로에 있어 이 글자를 담을 수 없다 —
  // 그래서 값만 여기 살고 조립기가 ko 판에서 `"Malgun Gothic"` 뒤에 붙인다.
  // {ko, en} 짝이 아닌 이유: 영어판에는 붙일 것이 없다(앞의 라틴 이름이 같은 폰트다).
  hangulFontAlias: "맑은 고딕",
};
