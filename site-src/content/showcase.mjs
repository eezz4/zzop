// 쇼케이스 — zzop 을 X(구 Twitter)/xAI 오픈소스 12개 레포에 한 번에 돌린 페이지의 문장들.
//
// 다른 content/*.mjs 와 다른 점이 하나 있다: 이 페이지는 **탭이 아니라 독립 문서**이고,
// 본문의 92% 가 산문이 아니라 데이터(mermaid 소스·NDJSON·캔버스 스크립트)다. 그래서 이 파일은
// 페이지를 통째로 들지 않고 **슬롯의 문장만** 든다 — 나머지 바이트는 site-src/showcase/page.html
// 이 그대로 소유하고, 조립기가 이 짝을 그 자리에 끼워 넣는다. 데이터를 언어별로 복사하지 않는
// 이유는 graph.html 슬라이스와 같다(1.architecture/distribution/site-generation.md §그래프 데이터):
// 좌표와 그래프 소스는 **그럴듯해 보이는 데이터**라 사본이 낡아도 사람 눈에 안 걸린다.
//
// 슬롯 id 는 `<태그><문서순서>` 다. 순서가 곧 id 라 문단을 옮기면 id 가 바뀌는데, 그것이 의도다 —
// 짝이 비면 빌드가 **슬롯 이름을 대면서** 죽으므로, 옮긴 사람이 그 자리에서 알게 된다.
//
// **`<code>` 안의 토큰은 두 판이 같아야 한다**(scripts/site/render.mjs 의 짝 검사). 독자가
// 타이핑하는 이름이라 번역 대상이 아니다 — 한국어 문장 안에서도 영어 그대로 남는다.
// 숫자도 마찬가지다: 측정값이고, 측정에는 언어가 없다.

export default {
  // <head> 의 문장들. 슬롯 id 가 `태그+순서`가 아닌 이유: 이 둘은 본문이 아니라 문서 자체의
  // 속성이고, 문단을 옮겨도 안 움직인다. `description` 은 속성값이라 태그도 따옴표도 못 들어간다.
  head: {
    title: {
      ko: `zzop 으로 본 X 의 오픈소스`,
      en: `zzop over X's open source`,
    },
    description: {
      ko: `X 와 xAI 가 공개한 레포 열두 개에 zzop v0.33.0 을 한 번에 돌렸다 — 파일 4,457개의 전체 의존 그래프를 한 캔버스에, 그리고 join·risk·posture·cochange 내보내기까지. 잘려 나간 것과 못 본 것은 전부 공시된다.`,
      en: `zzop v0.33.0 run over the twelve repositories X and xAI have open-sourced: the full 4,457-file dependency graph on one canvas, plus the join, risk, posture and co-change exports, every cap and blind spot disclosed.`,
    },
  },

  slots: {
    h11: {
      ko: `X 가 공개한 코드 전부에 zzop 을 돌렸다`,
      en: `zzop over everything X open-sourced`,
    },

    p2: {
      ko: `X(구 Twitter)와 xAI 의 레포 열두 개 &mdash; For You 피드, Grok 의 빌드 시스템,
    xAI SDK, 커뮤니티 노트 &mdash; 를 <strong>zzop v0.33.0</strong> 으로 2026-08-16 에
    <strong>한 번에</strong> 분석했다. 아래는 먼저 전체 의존 그래프를 한 캔버스에 올린 것이고,
    이어서 나머지 네 개 도메인 내보내기다. 잘려 나간 것과 못 본 것은 전부 그림 자체가 말한다.`,
      en: `Twelve repositories from X (formerly Twitter) and xAI &mdash; the For You feed,
    Grok's build system, the xAI SDK, community notes &mdash; analyzed in one run with
    <strong>zzop v0.33.0</strong> on 2026-08-16. Below is the full dependency graph on one canvas,
    then the four other domain exports. Every cap and every blind spot is disclosed in the diagram
    itself.`,
    },

    h23: {
      ko: `이 그림이 흔한 그래퍼와 다르게 생긴 이유`,
      en: `Why the shape is unlike a generic grapher's`,
    },

    p4: {
      ko: `흔한 그래퍼는 "모든 파일과 모든 참조"를 한 덩어리 털뭉치로 그린다. zzop 은 세 군데서
      갈린다. <strong>(1)</strong> 엣지는 <em>트리 안에서 실제로 풀린</em> import 뿐이다 &mdash;
      npm/pip 패키지 참조와 못 푼 스펙파이어는 버린다. 그래서 레포 열두 개가 서로 이어지지 않은
      <em>섬</em> 열두 개로 나온다(정직한 모양이다 &mdash; 이 레포들은 서로를 import 하지 않는다).
      <strong>(2)</strong> mermaid 뷰는 도메인마다 상한이 걸리는데, 상한이 버린 것은 문서 안에서
      공시된다 &mdash; 아래의 <code>drawn/in-scope/total</code> 줄이 그 계수다.
      <strong>(3)</strong> join 그래프의 노드는 파일이 아니라 <em>(source, side, kind, key)</em>
      관계다. 노드가 HTTP 라우트와 DB 테이블인 그림이 파일 그래프와 같은 모양일 수는 없다.`,
      en: `A typical grapher draws "every file and every reference" as one hairball. zzop differs three
      ways. <strong>(1)</strong> an edge is an import <em>resolved within the tree only</em> &mdash;
      npm/pip package references and unresolved specifiers are dropped, so twelve repositories come
      out as twelve unconnected <em>islands</em> (the honest shape: the repos do not import each
      other). <strong>(2)</strong> the mermaid views are capped per domain, and whatever a cap drops
      is disclosed inside the document &mdash; the <code>drawn/in-scope/total</code> lines below are
      that census. <strong>(3)</strong> the join graph's node is not a file but a
      <em>(source, side, kind, key)</em> relation, so a picture whose nodes are HTTP routes and DB
      tables cannot share a shape with a file graph in the first place.`,
    },

    h25: {
      ko: `전체 의존 그래프 &mdash; 섬 열두 개`,
      en: `Full dependency graph &mdash; twelve islands`,
    },

    p6: {
      ko: `<code>cosmograph</code> 내보내기를 손대지 않고 그대로 그린 것 &mdash; 파일 4,457개와
      트리 안 import 엣지 10,614개 전부, 상한 없음. 레포별로 뭉쳐 있고, 뭉치의 중심 쪽 노드일수록
      트리 안 연결이 많다. 점 크기는 LOC 이고,
      <span style="color:var(--accent)">빨간 테두리</span> 노드는 import 순환 안에 있다.
      파일 경로는 마우스를 올리면 나온다.`,
      en: `The whole <code>cosmograph</code> export drawn as-is: 4,457
    files and all 10,614 in-tree import edges, no cap. Clustered by
    repository; nodes toward a cluster's centre have the most in-tree connections; dot size is LOC;
    <span style="color:var(--accent)">red-outlined</span> nodes sit in an import cycle. Hover for the
    file path.`,
    },

    th7: {
      ko: `레포`,
      en: `repository`,
    },

    th8: {
      ko: `파일`,
      en: `files`,
    },

    th9: {
      ko: `엣지`,
      en: `edges`,
    },

    th10: {
      ko: `순환 안`,
      en: `in-cycle`,
    },

    th11: {
      ko: `loc`,
      en: `loc`,
    },

    p12: {
      ko: `zzop 이 스스로 낸 공시를 그대로 옮기면: 이 그래프는 <em>트리 안에서 풀린 엣지만</em>
      든다 &mdash; 수가 작다는 것은 "import 가 적다"가 아니라 "리졸버가 걸어 본 파일로 매핑하지
      못한 import 가 많다"일 수도 있다.`,
      en: `zzop's own disclosure, verbatim: this graph holds <em>resolved in-tree edges
    only</em> &mdash; a low number can mean "many imports the resolver could not map to a walked
    file", not "few imports".`,
    },

    h213: {
      ko: `join &mdash; 크로스레이어 조인(노드가 io 관계다)`,
      en: `join &mdash; the cross-layer join (nodes are io relations)`,
    },

    p14: {
      ko: `edges 2/2 &middot; unconsumedProvides 17/17 &middot; unprovidedConsumes 12/12 &middot;
      unresolvedConsumes 25/27 &middot; externalConsumes 1/1 &mdash; 버킷당 상한 25. 빠진 2개는
      문서가 스스로 공시한다`,
      en: `edges 2/2 &middot; unconsumedProvides 17/17 &middot; unprovidedConsumes 12/12
    &middot; unresolvedConsumes 25/27 &middot; externalConsumes 1/1 &mdash; cap 25/bucket; the 2
    dropped are disclosed by the document`,
    },

    summary15: {
      ko: `join 그래프 &mdash; mermaid 소스(아무 mermaid 뷰어에나 붙여 넣으면 된다)`,
      en: `join graph &mdash; mermaid source (paste into any mermaid viewer)`,
    },

    p16: {
      ko: `zzop 의 <code>--domain join</code> 출력 그대로다. 그 출력의 헤더가 스스로
      "mermaid 렌더러에 붙여 넣으라"고 적는다.`,
      en: `This is zzop's <code>--domain join</code> output verbatim; its own header says to paste it into a mermaid renderer.`,
    },

    figcaption17: {
      ko: `설정 하나로 레포 열두 개를 조인했다. 크로스레이어 엣지는 둘이 실제로 붙고(grok-build
      계열 안에서), 나머지는 버킷으로 분류된다 &mdash; grok-build 의 SQLite 테이블 열 개는 이
      코퍼스에 제공자가 없고(unprovided), 동적 URL 은 못 푼 채로 남는다. grok-1 은 조용히
      빠지는 대신 "조인이 못 보는 트리(공시된 실명)" 노드를 받는다.`,
      en: `Twelve repos joined under one config. Two cross-layer edges land (inside the
    grok-build family); the rest are classified by bucket &mdash; grok-build's ten SQLite tables have
    no provider in this corpus (unprovided), dynamic URLs stay unresolved. grok-1 gets a "not visible
    to the join (disclosed blindness)" node rather than a silent absence.`,
    },

    h218: {
      ko: `dep &mdash; import 그래프(mermaid 뷰, --top 40 노드)`,
      en: `dep &mdash; import graph (mermaid view, --top 40 nodes)`,
    },

    p19: {
      ko: `nodes 40/4457 &middot; edges 63/10614 &middot; cycles 6 &mdash; 40 은 읽히게 하려는
      상한이고, 전체는 위의 캔버스다`,
      en: `nodes 40/4457 &middot; edges 63/10614 &middot; cycles 6 &mdash; 40 is the
    readability cap; the full set is the canvas above`,
    },

    summary20: {
      ko: `dep 그래프 &mdash; mermaid 소스(아무 mermaid 뷰어에나 붙여 넣으면 된다)`,
      en: `dep graph &mdash; mermaid source (paste into any mermaid viewer)`,
    },

    p21: {
      ko: `zzop 의 <code>--domain dep</code> 출력 그대로다. 그 출력의 헤더가 스스로
      "mermaid 렌더러에 붙여 넣으라"고 적는다.`,
      en: `This is zzop's <code>--domain dep</code> output verbatim; its own header says to paste it into a mermaid renderer.`,
    },

    h222: {
      ko: `risk &mdash; 허브와 추출 이음새`,
      en: `risk &mdash; hubs and extraction seams`,
    },

    p23: {
      ko: `hubs 12/119 &middot; seams 12/30 &mdash; 종류별 상한 12`,
      en: `hubs 12/119 &middot; seams 12/30 &mdash; per-kind cap 12`,
    },

    summary24: {
      ko: `risk 그래프 &mdash; mermaid 소스(아무 mermaid 뷰어에나 붙여 넣으면 된다)`,
      en: `risk graph &mdash; mermaid source (paste into any mermaid viewer)`,
    },

    p25: {
      ko: `zzop 의 <code>--domain risk</code> 출력 그대로다. 그 출력의 헤더가 스스로
      "mermaid 렌더러에 붙여 넣으라"고 적는다.`,
      en: `This is zzop's <code>--domain risk</code> output verbatim; its own header says to paste it into a mermaid renderer.`,
    },

    h226: {
      ko: `posture &mdash; 변경 가능한 공격 표면과 그 가드 상태`,
      en: `posture &mdash; mutating attack surface and its guard status`,
    },

    p27: {
      ko: `mutating routes 6/6 &middot; reported no-auth-evidence 3 &mdash; 깃발 모양은 가드가
      없다는 뜻이고, 상자는 "가드됐거나 면제"이지 <strong>가드됐음이 증명된 것이 아니다</strong>
      (룰은 자기가 판정할 수 없는 것에 대해 침묵한다)`,
      en: `mutating routes 6/6 &middot; reported no-auth-evidence 3 &mdash; a flag shape
    is unguarded; a box is "guarded-or-exempt", NOT proven guarded (the rule stays silent on what it
    cannot judge)`,
    },

    summary28: {
      ko: `posture 그래프 &mdash; mermaid 소스(아무 mermaid 뷰어에나 붙여 넣으면 된다)`,
      en: `posture graph &mdash; mermaid source (paste into any mermaid viewer)`,
    },

    p29: {
      ko: `zzop 의 <code>--domain posture</code> 출력 그대로다. 그 출력의 헤더가 스스로
      "mermaid 렌더러에 붙여 넣으라"고 적는다.`,
      en: `This is zzop's <code>--domain posture</code> output verbatim; its own header says to paste it into a mermaid renderer.`,
    },

    figcaption30: {
      ko: `깃발이 붙은 라우트 셋은 전부 grok-build 의 ptyctl &mdash; 로컬 PTY 제어용 개발 서버다.
      zzop 이 보고하는 것은 "인증 증거가 없다"이지 "취약하다"가 아니다.`,
      en: `The three flagged routes are grok-build's ptyctl &mdash; a local PTY-control dev
    server. "No auth evidence" is what zzop reports, not "vulnerable".`,
    },

    h231: {
      ko: `cochange &mdash; git 이력에서 같이 바뀌는 파일들`,
      en: `cochange &mdash; files that change together in git history`,
    },

    p32: {
      ko: `edges 30/275 &mdash; 파일을 2~25개 건드린 커밋만 짝을 만들고, 파일마다 가장 강한
      상대만 남긴다. 표본이지 레포 전체 합이 아니다`,
      en: `edges 30/275 &mdash; only commits touching 2 to 25 files form a pair, and each
    file keeps its strongest partners; a sample, never a repository total`,
    },

    summary33: {
      ko: `cochange 그래프 &mdash; mermaid 소스(아무 mermaid 뷰어에나 붙여 넣으면 된다)`,
      en: `cochange graph &mdash; mermaid source (paste into any mermaid viewer)`,
    },

    p34: {
      ko: `zzop 의 <code>--domain cochange</code> 출력 그대로다. 그 출력의 헤더가 스스로
      "mermaid 렌더러에 붙여 넣으라"고 적는다.`,
      en: `This is zzop's <code>--domain cochange</code> output verbatim; its own header says to paste it into a mermaid renderer.`,
    },

    h235: {
      ko: `재현하기`,
      en: `Reproduce`,
    },

    p36: {
      ko: `레포 열두 개를 클론한 뒤 도메인마다 커맨드 하나씩:
      <code>zzop graph --config &lt;config&gt; --domain join|dep|risk|posture|cochange</code>.
      전체 그래프는 <code>cosmograph</code> 레인이다 &mdash;
      <code>zzop graph --config &lt;config&gt; --domain dep --format cosmograph-nodes</code> 와
      <code>--format cosmograph-links</code> 가 이 캔버스가 읽는 NDJSON 표 두 개를 낸다.
      발견 계수(레포 네 개에 걸쳐 171건)는 <code>zzop cross --config &lt;config&gt;</code> 가
      낸 것이고, 그 정직-공시 관점은 <a href="index.html">개요</a>에 있다.`,
      en: `Clone the twelve repositories, then one command per domain:
    <code>zzop graph --config &lt;config&gt; --domain join|dep|risk|posture|cochange</code>. The full
    graph is the <code>cosmograph</code> lane:
    <code>zzop graph --config &lt;config&gt; --domain dep --format cosmograph-nodes</code> and
    <code>--format cosmograph-links</code>, which emit the two NDJSON tables this canvas reads. The
    findings census (171 across four repos) comes from <code>zzop cross --config &lt;config&gt;</code>;
    its honest-disclosure framing is on the <a href="index.html">overview</a>.`,
    },

    stat37: {
      ko: `dep 노드(파일)`,
      en: `dep nodes (files)`,
    },

    stat38: {
      ko: `트리 안에서 풀린 import 엣지`,
      en: `in-tree import edges`,
    },

    stat39: {
      ko: `import 순환`,
      en: `import cycles`,
    },

    stat40: {
      ko: `LOC(dep 노드)`,
      en: `LOC (dep nodes)`,
    },

    stat41: {
      ko: `발견(12개 중 4개 레포)`,
      en: `findings (4 of 12 repos)`,
    },

    stat42: {
      ko: `변경 라우트 / 인증 증거 없음`,
      en: `mutating routes / no auth evidence`,
    },

    eyebrow43: {
      ko: `실전`,
      en: `In the field`,
    },

  },
};
