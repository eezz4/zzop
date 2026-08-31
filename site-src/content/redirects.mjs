// 리다이렉트 스텁 두 장(`usage.html` · `architecture.html`)의 문장들.
//
// 이 두 장에는 자기 내용이 없다. 옛 페이지가 index 의 탭으로 접힌 뒤, 이미 발행된 링크와 북마크를
// 받아 주려고 남은 파일이다. 그래서 다른 다섯 장과 달리 **번역할 산문이 거의 없고**, 대신 번역하지
// 않으면 생기는 문제가 있었다: 한국어 문서 내비게이션의 `Usage`/`How it works` 가 영어 스텁으로
// 가고, 그 스텁이 **영어 index** 로 넘겼다. URL 이 곧 언어라는 이 사이트의 규약에서, 한국어 경로를
// 따라간 독자가 영어로 떨어지는 유일한 구멍이 여기였다.
//
// **`refresh` 는 이 파일에 없다** — 두 판 모두 `index.html#p-...` 라는 같은 상대 URL 로 넘기고,
// `site/ko/usage.html` 에서 그것은 `site/ko/index.html` 을 가리킨다. 즉 판을 가르는 것은 문장이
// 아니라 파일이 사는 자리다. `canonical` 은 반대로 판마다 다르다: 절대 URL 이라 크롤러에게
// 어느 쪽 index 가 이 내용의 주인인지 말해야 하고, 상대 경로로는 그 말을 할 수 없다.
//
// ⚠ `en` 은 **이미 발행된 문장 그대로**다. 영어 두 장은 이 템플릿에서 나온 결과가 커밋된 파일과
// 바이트까지 같음을 확인하고 나서야 한국어를 붙였다.

export default {
  // 두 페이지가 같은 제목을 찍는다. 한 번만 적어 두 페이지가 함께 움직이게 한다 — 스텁의 제목이
  // 서로 달라야 할 이유가 생기면 그때 갈라야지, 미리 두 벌로 두면 한쪽만 고쳐진다.
  h1: {
    ko: `이 페이지는 옮겨졌습니다`,
    en: `This page moved`,
  },

  pages: {
    "usage.html": {
      title: {
        ko: `zzop — 사용법 (옮겨짐)`,
        en: `zzop — Usage (moved)`,
      },
      description: {
        ko: `사용법 페이지는 개요 페이지 안으로 옮겨져 그곳의 Usage 탭이 되었습니다.`,
        en: `The usage page moved into the overview page, where it is the Usage tab.`,
      },
      // 개발자가 읽는 HTML 주석. 발행된 페이지에 그대로 실리므로 여기가 주인이다.
      stubNote: {
        ko: `<!-- 리다이렉트 스텁. 이 페이지에는 더 이상 자기 내용이 없다: 사용법 자료는 index.html 의
     탭이고, 같은 사실을 두 페이지가 가지면 그중 하나는 반드시 낡는다.`,
        en: `<!-- REDIRECT STUB. This page has no content of its own any more: the usage material is a tab on
     index.html, and two pages owning the same facts means one of them is always the stale one.`,
      },
      moved: {
        ko: `여기 있던 것은 전부 개요 페이지의 <strong>Usage</strong> 탭으로 갔습니다. 자동으로 넘어가지 않으면 <a href="index.html#p-usage">index.html#p-usage</a> 를 따라가세요.`,
        en: `Everything that was here is now the <strong>Usage</strong> tab of the overview page. If you are not redirected automatically, follow <a href="index.html#p-usage">index.html#p-usage</a>.`,
      },
    },

    "architecture.html": {
      title: {
        ko: `zzop — 아키텍처 (옮겨짐)`,
        en: `zzop — Architecture (moved)`,
      },
      description: {
        ko: `아키텍처 페이지는 개요 페이지 안으로 옮겨져 그곳의 How it works 탭이 되었습니다.`,
        en: `The architecture page moved into the overview page, where it is the How it works tab.`,
      },
      stubNote: {
        ko: `<!-- 리다이렉트 스텁. 이 페이지에는 더 이상 자기 내용이 없다: 아키텍처 자료는 index.html 의
     탭이고, 같은 사실을 두 페이지가 가지면 그중 하나는 반드시 낡는다.`,
        en: `<!-- REDIRECT STUB. This page has no content of its own any more: the architecture material is a tab
     on index.html, and two pages owning the same facts means one of them is always the stale one.`,
      },
      moved: {
        ko: `여기 있던 것은 전부 개요 페이지의 <strong>How it works</strong> 탭으로 갔습니다. 자동으로 넘어가지 않으면 <a href="index.html#p-arch">index.html#p-arch</a> 를 따라가세요.`,
        en: `Everything that was here is now the <strong>How it works</strong> tab of the overview page. If you are not redirected automatically, follow <a href="index.html#p-arch">index.html#p-arch</a>.`,
      },
    },
  },
};
