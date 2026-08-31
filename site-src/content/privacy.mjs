// 개인정보 처리방침 — zzop 이 무엇을 수집하지 않는가.
//
// 이 페이지의 한국어는 다른 페이지보다 **직역에 가깝다**. 이 사이트의 번역 계약은 보통
// "번역이 아니라 다시 쓴다"이지만(1.architecture/distribution/site-generation.md), 방침 문서에서
// 다시 쓰는 것은 **약속의 범위를 바꾸는 것**이다. 그래서 여기서는 문장 구조를 옮기고,
// 특히 **부정문의 범위**(무엇이 "아무것도"이고 무엇이 "그 문장에 안 덮이는가")를 원문과 같게 둔다.
// li6 이 그 이유의 전부다: 바이너리는 네트워크를 아예 안 쓰지만 플러그인 훅은 쓴다는 것을
// 원문이 **묻지 않고 세어서** 적는데, 한국어가 그 구분을 흐리면 더 강한 약속이 되어 버린다.
//
// URL·`<code>` 토큰은 번역 대상이 아니다 — 독자가 클릭하거나 타이핑하는 것이다.

export default {
  // ⚠ `en` 쪽은 **이미 발행된 문장 그대로**다. 이 짝을 만들면서 영어 제목·설명을 더 낫게 다시
  // 썼다가 되돌렸다: 이 페이지를 이중언어로 만드는 일은 한국어를 **더하는** 것이지 영어를 바꾸는
  // 것이 아니고, 검색 결과에 이미 나가 있는 문장을 부수 효과로 갈아치우면 그 변경은 아무 커밋
  // 메시지도 설명하지 않는다. 영어를 고치고 싶으면 그것은 별개의 판정이다.
  head: {
    title: {
      ko: `zzop — 개인정보 처리방침`,
      en: `zzop — Privacy`,
    },
    description: {
      ko: `zzop 개인정보 처리방침 — 분석은 전부 로컬에서 돌고, 바이너리는 네트워크 요청을 하지 않으며, 텔레메트리도 개인정보 수집도 없다.`,
      en: `zzop privacy policy — all analysis is local, the binaries make no network requests, and no telemetry or personal data is collected.`,
    },
  },

  slots: {
    h11: {
      ko: `개인정보 처리방침`,
      en: `Privacy Policy`,
    },

    p2: {
      ko: `<strong>zzop 은 아무것도 수집하지 않는다.</strong> 짧게 말하면: 당신의 코드는 당신의 기기를 떠나지 않고, 당신이나 당신의 사용에 관한 어떤 데이터도 zzop 이 모으거나 전송하거나 저장하지 않는다.`,
      en: `<strong>zzop collects nothing.</strong> The short version: your code never leaves your machine, and no data about you or your usage is gathered, transmitted, or stored by zzop.`,
    },

    h23: {
      ko: `zzop 이 당신의 데이터로 하는 일`,
      en: `What zzop does with your data`,
    },

    li4: {
      ko: `<strong>분석은 전부 로컬이다.</strong> <code>zzop</code> CLI, <code>zzop-mcp</code> MCP 서버, npm 패키지 전부 당신의 기기에서만 돌고, 당신이 가리킨 레포지토리 경로만 읽는다.`,
      en: `<strong>All analysis is local.</strong> The <code>zzop</code> CLI, the <code>zzop-mcp</code> MCP server, and the npm packages run entirely on your machine and read only the repository paths you point them at.`,
    },

    li5: {
      ko: `<strong>네트워크 접근이 없다.</strong> 분석 바이너리는 어떤 종류의 네트워크 요청도 하지 않는다 — 설계가 그렇고, HTTP 의존성이 0개다. 소스와 의존성 트리에서 확인할 수 있다.`,
      en: `<strong>No network access.</strong> The analysis binaries make no network requests of any kind — by design, they carry zero HTTP dependencies. This is verifiable from the source and the dependency tree.`,
    },

    li6: {
      ko: `<strong>텔레메트리가 없다.</strong> 사용 추적도, 크래시 리포팅도, 애널리틱스도 없고, 당신이나 당신의 코드에 관한 것은 어디로도 안 나간다 — 바이너리도, MCP 서버도, Claude Code 플러그인도. <em>플러그인 자신의</em> 네트워크 활동은 그 문장에 안 덮이며, 묻어 두는 대신 여기서 센다: <code>SessionStart</code> 훅이 <code>api.github.com</code> 에 최신 릴리스 태그를 묻는다 — 세션마다 한 번 — 새 버전이 있다는 것을 <em>보고</em>하기 위해서다(보고만 하고 절대 적용하지 않는다). 설치 첫 회에는 같은 훅이 GitHub Releases 에서 서버 바이너리와 <code>SHA256SUMS</code> 를 받는다 — 요청 두 번 더, 설치 시점에만, 아래 설치 절이 다루는 것과 같은 접촉이다. 그리고 플러그인의 <code>/zzop:update</code> 슬래시 커맨드는 당신이 실행할 때마다 최신 릴리스 요청을 한 번 반복한다. 세션마다 반복되는 그 요청은 당신의 IP 를 실어 나르고 세션이 시작됐다는 사실을 드러낸다. 그 외에는 아무것도 싣지 않는다. 분석 바이너리 자체는 네트워크 요청을 전혀 하지 않는다.`,
      en: `<strong>No telemetry.</strong> There is no usage tracking, no crash reporting, no analytics, and nothing about you or your code is ever sent anywhere — not by the binaries, not by the MCP server, not by the Claude Code plugin. The plugin's own network activity is not covered by that sentence and is counted here rather than buried: its <code>SessionStart</code> hook asks <code>api.github.com</code> for the latest release tag — one request, on every session — so it can <em>report</em> that a newer version exists (it reports updates, never applies them); on first install the same hook downloads the server binary and its <code>SHA256SUMS</code> from GitHub Releases — two more requests, install-time only, the same touches the install section below covers; and the plugin's <code>/zzop:update</code> slash command, when you run it, repeats the latest-release request once per invocation. The recurring per-session request carries your IP and reveals that a session started; it carries nothing else. The analysis binaries themselves make no network requests at all.`,
    },

    li7: {
      ko: `<strong>계정도 키도 없다.</strong> zzop 은 가입도, API 키도, 자격증명도 요구하지 않는다.`,
      en: `<strong>No accounts, no keys.</strong> zzop requires no sign-up, no API key, and no credentials.`,
    },

    li8: {
      ko: `<strong>MCP 통신은 로컬에 머문다.</strong> MCP 서버는 당신의 기기에서 당신의 MCP 클라이언트(예: Claude Code / Claude Desktop)와 stdio 로 JSON-RPC 를 주고받는다. 그 클라이언트가 분석 결과를 가지고 무엇을 하는지는 <strong>이 방침이 아니라 그 클라이언트의 방침</strong>이 정한다.`,
      en: `<strong>MCP communication stays local.</strong> The MCP server speaks JSON-RPC over stdio with your MCP client (e.g. Claude Code / Claude Desktop) on your machine. What that client then does with analysis results is governed by that client's own privacy policy, not this one.`,
    },

    h29: {
      ko: `설치하면서 닿을 수 있는 외부 서비스`,
      en: `Third-party services you may touch while installing`,
    },

    p10: {
      ko: `릴리스 바이너리나 npm 패키지를 받는 일에는 GitHub 과 npm 의 인프라가 관여하고, 그것은 각자의 방침이 정한다:`,
      en: `Downloading release binaries or npm packages involves GitHub and npm's infrastructure, governed by their own policies:`,
    },

    li11: {
      ko: `GitHub (릴리스, 레포지토리): <a href="https://docs.github.com/en/site-policy/privacy-policies" target="_blank" rel="noreferrer">GitHub privacy policies</a>`,
      en: `GitHub (releases, repository): <a href="https://docs.github.com/en/site-policy/privacy-policies" target="_blank" rel="noreferrer">GitHub privacy policies</a>`,
    },

    li12: {
      ko: `npm (패키지 레지스트리): <a href="https://docs.npmjs.com/policies/privacy" target="_blank" rel="noreferrer">npm privacy policy</a>`,
      en: `npm (package registry): <a href="https://docs.npmjs.com/policies/privacy" target="_blank" rel="noreferrer">npm privacy policy</a>`,
    },

    h213: {
      ko: `변경`,
      en: `Changes`,
    },

    p14: {
      ko: `zzop 의 동작이 이 방침에 영향을 주는 방식으로 바뀌면, 이 페이지는 <strong>같은 커밋에서</strong> 함께 바뀐다 — 공개 레포지토리 이력에 남는다.`,
      en: `If zzop's behavior ever changes in a way that affects this policy, this page changes in the same commit, in the public repository history.`,
    },

    h215: {
      ko: `문의`,
      en: `Contact`,
    },

    p16: {
      ko: `질문은 <a href="https://github.com/eezz4/zzop/issues" target="_blank" rel="noreferrer">github.com/eezz4/zzop/issues</a> 에 이슈로 남기면 된다.`,
      en: `Questions: open an issue at <a href="https://github.com/eezz4/zzop/issues" target="_blank" rel="noreferrer">github.com/eezz4/zzop/issues</a>.`,
    },

  },
};
