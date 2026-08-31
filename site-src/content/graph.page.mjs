// 그래프 페이지(독립 `site/graph.html`)의 문장들.
//
// ⚠ 이 파일은 `graph.mjs` 와 다르다. 그쪽은 **index 의 그래프 탭**(SPA 섹션 `#p-graph`)이고,
// 이쪽은 캔버스가 실제로 사는 **독립 페이지**다. 같은 그림을 두 자리가 소개하므로 문장이 비슷해
// 보이지만 주인은 서로 다르고, 한쪽을 고쳐도 다른 쪽은 안 따라온다.
//
// **lede 문단은 이 파일에 없다** — 템플릿에 영어 그대로 남아 두 판에 같이 실린다. 판정이다(2026-08-18 사용자: *"그래프 내부는 영어로
// 표시해도 돼"*). 그 문장 안의 `1,513 nodes, 2,913 edges` 는 `scripts/site-graph-data.mjs` 가
// **영어 정규식으로 갈아끼우는** 값이다. 한국어로 옮기면 그 정규식이 한국어 문장도 찾아야 하고,
// 안 맞을 때 실패하는 게 아니라 **숫자가 낡은 채 통과**한다 — 이 레포가 cardinal 이라 부르는 모양이다.
// 문장을 영어로 두면 생성기는 템플릿에서 그대로 찾고, 두 판이 그 템플릿에서 나온다.
// 캔버스가 그리는 글자(파일 경로·도메인 이름)도 같은 이유로 데이터이지 번역 대상이 아니다.

export default {
  // ⚠ `en` 은 **이미 발행된 문장 그대로**다. `head` 는 본문과 달리 페이지에서 자동으로 뽑혀 오지
  // 않으므로(슬롯이 아니라 손으로 쓴다) 여기서 영어를 "더 낫게" 다시 쓰기가 쉽고, privacy 와 graph
  // 두 페이지에서 실제로 그렇게 했다가 되돌렸다. 페이지를 이중언어로 만드는 것은 한국어를 **더하는**
  // 일이다 — 검색 결과에 이미 나가 있는 제목을 부수 효과로 갈아치우면 어떤 커밋 메시지도 그 변경을
  // 설명하지 않는다. 영어를 고치고 싶으면 그것은 별개의 판정이다.
  head: {
    title: {
      ko: `zzop — 의존 그래프`,
      en: `zzop — Dependency graph`,
    },
    description: {
      ko: `zzop 을 zzop 자신에게 돌렸다: 이 레포지토리의 모든 파일과 그 사이의 모든 import 를 상한 없이. 엔진은 NDJSON 표 두 개를 낼 뿐 아무것도 그리지 않고, 그림은 뷰어가 그것을 읽어 그린다.`,
      en: `zzop rendered against itself: every file in the repository and every import between them, uncapped. The engine emits two NDJSON tables and draws nothing; a viewer reads them.`,
    },
  },

  slots: {
    p1: {
      ko: `아키텍처 · 그림은 남이 그린다`,
      en: `Architecture · rendered by someone else`,
    },

    h12: {
      ko: `zzop 자신의 import 그래프, 상한 없이`,
      en: `zzop's own import graph, uncapped`,
    },

    p4: {
      ko: `스냅숏이다: 이 그림과 이 페이지의 모든 수치는 페이지를 재생성할 때마다
      <code>scripts/site-graph-data.mjs</code> 가 다시 쓴다. 즉 <strong>그 재생성이 돌았던 트리</strong>를
      설명한다. CI 가 이걸 걸어 두지 않았으므로 레포는 그 뒤로 움직였다. 지금 체크아웃에 대고 위 커맨드
      둘을 다시 돌리는 것이 곧 재계수다.`,
      en: `Snapshot: this drawing and every count on this page are rewritten by
      <code>scripts/site-graph-data.mjs</code> each time the page is regenerated, so they describe the tree
      that regeneration ran against. Nothing wires it to CI, so the repository has moved since.
      Re-running the two commands above against the current checkout is the recount.`,
    },

    figcaption5: {
      ko: `<span id="gcensus"></span> <span class="graph-src">위 줄은 그 커맨드가 stderr 에 찍는
      것이다 — stdout 은 파싱 가능한 표로 남는다. 레이아웃은 미리 계산돼 고정돼 있으므로 이 그림은
      언제 와도 같다.</span>`,
      en: `
      <span id="gcensus"></span>
      <span class="graph-src">The line above is what the command prints on stderr — stdout stays a
      parseable table. Layout is precomputed and fixed, so this picture is the same on every visit.</span>
    `,
    },

    h26: {
      ko: `읽는 법`,
      en: `Reading it`,
    },

  },
};
