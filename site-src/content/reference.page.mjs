// 레퍼런스 페이지(`site/reference.html` · `site/ko/reference.html`)의 문장들.
//
// 이 페이지는 **계약서**다. 다른 네 장과 달리 문장 대부분이 필드 이름·타입·와이어 모양을 말하므로,
// 번역에서 지켜야 할 것이 프로즈보다 **식별자**다: `<code>` 토큰과 `href` 는 번역 대상이 아니라
// 목적지이고, 하나라도 바뀌면 그것은 번역된 문장이 아니라 **다른 문장**이다. 빌더가 슬롯마다
// 두 판의 `<code>` 다중집합과 `href` 집합이 같은지 확인하고, 다르면 쓰지 않는다.
//
// **시그니처만 있는 슬롯은 두 판이 같은 글자를 찍는다**(`(config_json: &str) -> Result<String, String>`,
// 타입 이름 `AnalyzeRequest`). 한국어로 옮길 프로즈가 없어서지 빠뜨린 것이 아니다 — 그런 슬롯에는
// 그렇게 적어 뒀다.
//
// ⚠ `en` 은 **이미 발행된 문장 그대로**다. `head` 는 본문과 달리 페이지에서 자동으로 뽑혀 오지 않으므로
// 여기서 영어를 "더 낫게" 다시 쓰기가 쉽고, 앞선 페이지 두 곳에서 실제로 그렇게 했다가 되돌렸다.
// 페이지를 이중언어로 만드는 것은 한국어를 **더하는** 일이다 — 검색 결과에 이미 나가 있는 제목을
// 부수 효과로 갈아치우면 어떤 커밋 메시지도 그 변경을 설명하지 않는다. 영어를 고치고 싶으면 그것은
// 별개의 판정이다.

export default {
  head: {
    title: {
      ko: `zzop — 레퍼런스 (입력 &amp; 출력 계약)`,
      en: `zzop — Reference (input &amp; output contract)`,
    },
    description: {
      ko: `zzop 의 모든 표면이 공유하는 하나의 JSON 계약 — CLI 서브커맨드, MCP 도구, 인프로세스 호출: AnalyzeRequest 입력 모양, AnalyzeOutputView 출력 모양, verdict/coverage/disclosure 어휘, 그리고 다중 레포 분석.`,
      en: `The JSON contract every zzop surface shares — CLI subcommands, MCP tools, in-process: the AnalyzeRequest input shape, the AnalyzeOutputView output shape, the verdict/coverage/disclosure vocabulary, and multi-repo analysis.`,
    },
  },

  slots: {
    h11: {
      ko: `JSON 계약`,
      en: `The JSON contract`,
    },

    p2: {
      ko: `zzop 은 JSON 계약을 하나만 노출하고, 모든 표면이 그 하나를 말한다 — <code>zzop</code> CLI 서브커맨드, <code>zzop-mcp</code> MCP 도구, 인프로세스 호출이 전부 같은 요청 모양을 보내고 같은 출력 모양을 돌려받는다. 이 페이지가 <em>곧</em> 그 계약이다: <a href="#config-reference">입력</a>(<code>AnalyzeRequest</code>), <a href="#output-schema">출력</a>(<code>AnalyzeOutputView</code>), 그리고 그 사이의 어휘. zzop 을 그냥 <em>돌리는</em> 법은 <a href="usage.html">Usage</a> 를 보라. 임베드하려면 바이너리의 JSON 서브커맨드로 셸아웃한다 — JSON 넣고 JSON 받고, 링크는 없다. 이것이 빌드된 <code>zzop-facade</code> / <code>zzop-summary</code> 크레이트는 워크스페이스 내부용이라(crates.io 에 발행하지 않는다) 인프로세스 Rust 의존이란 <code>cargo add</code> 가 아니라 워크스페이스를 벤더링한다는 뜻이다.`,
      en: `zzop exposes one JSON contract, and every surface speaks it — the <code>zzop</code> CLI subcommands, the <code>zzop-mcp</code> MCP tools, and an in-process call all send the same request shape and get back the same output shape. This page <em>is</em> that contract: the <a href="#config-reference">input</a> (<code>AnalyzeRequest</code>), the <a href="#output-schema">output</a> (<code>AnalyzeOutputView</code>), and the vocabulary in between. To just <em>run</em> zzop, see <a href="usage.html">Usage</a>. To embed it, shell out to the binary's JSON subcommands — JSON in, JSON out, no linkage; the <code>zzop-facade</code> / <code>zzop-summary</code> crates it is built from are workspace-internal (not published to crates.io), so an in-process Rust dependency means vendoring the workspace, not <code>cargo add</code>.`,
    },

    h23: {
      ko: `공유되는 오퍼레이션`,
      en: `The shared operations`,
    },

    p4: {
      ko: `아래의 모든 오퍼레이션은 최소 두 갈래로 닿는다 — <code>zzop</code> CLI 서브커맨드와 인프로세스 호출 — 그리고 그 둘은 하나의 구현을 공유한다(<code>crates/facade</code>, 그리고 CLI 와 MCP 도구가 받는 설정 자동탐색·결과 정형을 맡는 <code>crates/summary</code>). 어느 표면을 쓰든 같은 요청을 넣으면 같은 JSON 이 나오고, <code>version</code> 을 빼면 전부 JSON 문자열 입력 / JSON 문자열 출력이다. <strong><code>zzop-mcp</code> MCP 도구가 세 번째 갈래이고, 모든 오퍼레이션에 하나씩 있는 것은 아니다</strong> — 아래 각 행은 자기 오퍼레이션이 닿는 표면을 스스로 밝히고, MCP 도구가 없는 행은 없다는 사실과 그 이유를 함께 적는다. 메워야 할 구멍이 아니라 의도된 비대칭이다: CLI 전용 레인마다 짝이 없는 이유가 <code>docs/contracts/surface-parity.json</code>(<code>_cliOnlyLanes</code>)에 레인 단위로 기록돼 있고, 메타 테스트(<code class="code-wrap">crates/engine/tests/rule_contracts/surface_parity.rs</code>)가 그것을 읽어 MCP 응답을 그 등기부가 말하는 것에 붙들어 둔다.`,
      en: `Every operation below is reached at least two ways — a <code>zzop</code> CLI subcommand and an in-process call — over one shared implementation (<code>crates/facade</code>, plus <code>crates/summary</code> for the config auto-discovery and result-shaping the CLI and MCP tools get). Same request in, same JSON out, whichever surface you use; all are JSON-string-in / JSON-string-out except <code>version</code>. <strong>A <code>zzop-mcp</code> MCP tool is the third way, and not every operation has one</strong> — each row below names the surfaces its own operation reaches, and a row with no MCP tool says so and says why. That is a deliberate asymmetry rather than a gap to close: the reason each CLI-only lane has no twin is recorded per lane in <code>docs/contracts/surface-parity.json</code> (<code>_cliOnlyLanes</code>), which a meta-test (<code class="code-wrap">crates/engine/tests/rule_contracts/surface_parity.rs</code>) reads to hold the MCP reply to what that registry says it carries.`,
    },

    h35: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>analyze_json</code>`,
      en: `<code>analyze_json</code>`,
    },

    p6: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(config_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(config_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p7: {
      ko: `<code>AnalyzeRequest</code> &rarr; <code>AnalyzeOutputView</code>. 트리 하나를 분석한다. <code>zzop analyze &lt;path&gt;</code> / <code>analyze --config &lt;path&gt;</code> CLI 서브커맨드와 <code>analyze_repo</code> MCP 도구(<code>zzop-summary</code> 경유)를 떠받친다.`,
      en: `<code>AnalyzeRequest</code> &rarr; <code>AnalyzeOutputView</code>. Analyzes one tree. Backs the <code>zzop analyze &lt;path&gt;</code> / <code>analyze --config &lt;path&gt;</code> CLI subcommand and the <code>analyze_repo</code> MCP tool (via <code>zzop-summary</code>).`,
    },

    h38: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>analyze_trees_json</code>`,
      en: `<code>analyze_trees_json</code>`,
    },

    p9: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(config_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(config_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p10: {
      ko: `<code>AnalyzeTreesRequest</code>(<code>{ trees: AnalyzeRequest[] }</code>) &rarr; <code>MultiAnalyzeOutputView</code>. 여러 트리를 분석하고 레이어를 가로질러 잇는다. <code>zzop cross</code> / <code>cross_repo</code> MCP 도구를 떠받친다.`,
      en: `<code>AnalyzeTreesRequest</code> (<code>{ trees: AnalyzeRequest[] }</code>) &rarr; <code>MultiAnalyzeOutputView</code>. Analyzes several trees and joins them cross-layer. Backs <code>zzop cross</code> / the <code>cross_repo</code> MCP tool.`,
    },

    h311: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>analyze_envelope_json</code>`,
      en: `<code>analyze_envelope_json</code>`,
    },

    p12: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(envelope_json: &amp;str, config_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(envelope_json: &amp;str, config_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p13: {
      ko: `<code>NormalizedEnvelope</code> + <code>EnvelopeAnalyzeRequest</code> &rarr; <code>AnalyzeOutputView</code>. 외부 파서 어댑터가 만든 Normalized AST 엔벨로프를 분석한다. <code>zzop analyze-envelope</code> / <code>analyze_envelope</code> MCP 도구를 떠받친다.`,
      en: `<code>NormalizedEnvelope</code> + <code>EnvelopeAnalyzeRequest</code> &rarr; <code>AnalyzeOutputView</code>. Analyzes a Normalized AST envelope produced by an external parser adapter. Backs <code>zzop analyze-envelope</code> / the <code>analyze_envelope</code> MCP tool.`,
    },

    h314: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>query_io_json</code>`,
      en: `<code>query_io_json</code>`,
    },

    p15: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(analysis_json: &amp;str, query_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(analysis_json: &amp;str, query_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p16: {
      ko: `<code>analyze_trees_json</code> 출력 + <code>{ pattern: string }</code> &rarr; 엔드포인트/io 키에 대한 확정 답. 이미 만들어진 다중 트리 분석 위의 순수 후처리이고 재분석은 없다: <code>pattern</code> 은 모든 크로스레이어 io 키(HTTP 라우트, env 키, DB 테이블, 토픽, 그리고 풀리지 않은 consume 을 위한 <code>raw</code>)에 대소문자 무시 부분 문자열로 맞춰 보고, 결과는 봉인된 <code>verdict</code> 어휘(<code>linked</code> | <code>provided-only</code> | <code>consumed-unprovided</code> | <code>external</code> | <code>unresolved-only</code> | <code>ambiguous</code> | <code>mixed</code> | <code>not-found</code>)를 그 뒤의 매치·개수·관련 발견과 함께 싣는다. 질의가 망가졌을 때와 단일 트리 <code>analyze_json</code> 출력이 들어왔을 때는 에러를 낸다 — 안내가 붙은 에러다. verdict 는 조인 사실이기 때문이다(트리가 하나여도 조인하는 <code>analyze_trees_json</code> 을 돌려라). <code>zzop endpoint</code> / <code>check_endpoint</code> MCP 도구를 떠받친다.`,
      en: `<code>analyze_trees_json</code> output + <code>{ pattern: string }</code> &rarr; definitive endpoint/io-key answer. Pure post-processing over an already-produced multi-tree analysis — no re-analysis: <code>pattern</code> is case-insensitively substring-matched against every cross-layer io key (HTTP routes, env keys, DB tables, topics, plus <code>raw</code> for unresolved consumes), and the result carries a sealed <code>verdict</code> vocabulary (<code>linked</code> | <code>provided-only</code> | <code>consumed-unprovided</code> | <code>external</code> | <code>unresolved-only</code> | <code>ambiguous</code> | <code>mixed</code> | <code>not-found</code>) with the matches, counts, and related findings behind it. Errs on a malformed query and on single-tree <code>analyze_json</code> output — a guided error, since verdicts are join facts (run <code>analyze_trees_json</code>, which joins even a single tree). Backs <code>zzop endpoint</code> / the <code>check_endpoint</code> MCP tool.`,
    },

    h317: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>query_file_json</code>`,
      en: `<code>query_file_json</code>`,
    },

    p18: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(analysis_json: &amp;str, query_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(analysis_json: &amp;str, query_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p19: {
      ko: `<code>analyze_trees_json</code> 출력 + <code>{ path: string, sourceId?: string }</code> &rarr; zzop 이 파일 <strong>하나</strong>에 대해 아는 전부. <code>query_io_json</code> 과 같은 순수 후처리 계약이고 — 재분석은 없다 — 대상이 io 키 대신 파일 경로다: 그 파일이 속한 트리, 심볼, io 사실, <strong>양방향</strong> 의존 간선, 그리고 그 파일에 앵커된 모든 발견. <strong>상한이 없다</strong>, 의도적으로: 파일 하나는 유계이므로 버리는 것이 없고, 따라서 밝힐 절단도 없다.`,
      en: `<code>analyze_trees_json</code> output + <code>{ path: string, sourceId?: string }</code> &rarr; everything zzop knows about ONE file. The same pure-post-processing contract as <code>query_io_json</code> — no re-analysis — with a file path as the target instead of an io key: the tree it belongs to, its symbols, io facts, dependency edges in BOTH directions, and every finding anchored in it. <strong>Uncapped</strong>, deliberately: a single file is bounded, so nothing is dropped and there is no truncation to disclose.`,
    },

    p20: {
      ko: `이쪽의 봉인된 <code>verdict</code> 어휘(<code>analyzed</code> | <code>lexical-only</code> | <code>degraded</code> | <code>not-found</code>)가 답하는 것은 그 파일이 <strong>분석됐는가</strong>이지 건강한가가 아니다 — 빈 발견 목록은 첫 번째에서는 "깨끗하다"이고 다음 둘에서는 "구조 분석이 애초에 돌지 않았다"이다. <code>query_io_json</code> 과 마찬가지로 응답 자신의 <code>verdictMeaning</code> 필드가 돌려준 토큰의 정의를 싣고 다니므로, 어떤 문서도 그 정의의 두 번째 주인이 되지 않는다. <code>sourceId</code> 가 없으면 모든 트리를 뒤지고, 응답은 매치가 나온 트리를 이름 대며 나머지는 <code>otherTrees</code> 에 늘어놓는다 — 말없이 하나를 고르지 않는다. <code>not-found</code> 응답에는 워크된 경로 중 가장 가까운 것들이 <code>suggestions</code> 로 실린다. <code>path</code> 가 없는 질의와, 보고할 트리 정체성이 없는 단일 트리 <code>analyze_json</code> 출력에는 에러를 낸다. <code>zzop file</code> / <code>check_file</code> MCP 도구를 떠받친다.`,
      en: `Its sealed <code>verdict</code> vocabulary (<code>analyzed</code> | <code>lexical-only</code> | <code>degraded</code> | <code>not-found</code>) answers whether the file was ANALYZED, not whether it is healthy — an empty findings list means "clean" for the first and "nothing structural ever ran" for the next two; as with <code>query_io_json</code>, the reply's own <code>verdictMeaning</code> field carries the returned token's definition, so no document is a second owner of it. Without <code>sourceId</code> every tree is searched and the reply names the tree the match came from, listing the rest in <code>otherTrees</code> rather than picking silently; a <code>not-found</code> reply carries <code>suggestions</code>, the nearest walked paths. Errs on a query with no <code>path</code> and on single-tree <code>analyze_json</code> output, which has no tree identity to report. Backs <code>zzop file</code> / the <code>check_file</code> MCP tool.`,
    },

    h321: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>query_coverage_json</code>`,
      en: `<code>query_coverage_json</code>`,
    },

    p22: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(analysis_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(analysis_json: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p23: {
      ko: `<code>analyze_trees_json</code> 출력 &rarr; <strong>총량 가시성</strong> 뷰: &ldquo;zzop 이 이 트리를 실제로 얼마나 보고 있나?&rdquo; 트리마다 디스패치별 확장자 표(구조 / lexical-only / degraded, 그리고 <code>inDepGraph</code>), <code>blindSpots</code> &mdash; 컴파일된 각 룰의 시선을 그 트리의 구조 확장자 구성과 교차시킨 <strong>능력</strong> 축 &mdash; 그 트리 자신의 엔진 경고를 그대로 전달한 것(프레임워크 침묵 자기보고가 여기 실린다), 커버리지 센서스, <code>ioChannels</code>(<code>extracted</code> — 룰이 읽는 io 종류마다 한 행씩, <strong>0 이어도 반드시 실린다</strong>: 종류를 구분하지 않는 <code>joinContributionZero</code> 처럼 채워진 채널이 빈 채널을 대신 보증하지 못하게 한다. <code>zeroExtraction</code> — 이 빌드가 인식기를 가진 (채널, 확장자) 중 추출이 0 으로 돌아온 것을 이름 붙인 <strong>능력×실측</strong> 교차: 이 실행이 구조적으로 읽은 것 중 주요 비중인 확장자만 실리고(그 아래는 목록에서 <strong>빠지는 것이지 통과한 것이 아니다</strong>), 프레임워크 이름을 알아보는 게 아니라 트리 자체를 모집단으로 삼는 <strong>커버리지 사실</strong>이다), 그리고 <code>joinVisibility</code> 를 개수로(provides, consumesKeyed, consumesUnresolved) 의미와 함께 낸다 — 파생 비율은 내지 않는다. 몫은 1 중 1 에서나 440 중 400 에서나 똑같이 읽히기 때문이다. <strong>단일 점수는 일부러 없다</strong>: zzop 이 당신 트리에서 한 번도 재지 못한 축은, 그 사실 없이 인용될 숫자에 접혀 들어가는 대신 <strong>재지 못했다는 필드</strong>에 실려 간다. MCP 도구 짝이 없는 <code>zzop coverage</code> 를 떠받친다.`,
      en: `<code>analyze_trees_json</code> output &rarr; the AGGREGATE-VISIBILITY view: &ldquo;how much of this tree does zzop actually see?&rdquo; Per tree, an extension-by-dispatch table (structural / lexical-only / degraded, plus <code>inDepGraph</code>), <code>blindSpots</code> &mdash; the CAPABILITY axis, each compiled-in rule sightline crossed with the tree&rsquo;s structural extension mix &mdash; the tree&rsquo;s own engine warnings forwarded verbatim (the framework-silence self-reports ride there), the coverage census, <code>ioChannels</code> (<code>extracted</code> — one row per io kind the rules read, <strong>present even at zero</strong>, so a filled channel can no longer vouch for an empty one the way the kind-agnostic <code>joinContributionZero</code> does; <code>zeroExtraction</code> — the CAPABILITY&times;MEASURED cross naming each (channel, extension) this build has a recognizer for whose extraction came back 0, restricted to filetypes that are a principal share of what the run read structurally (one under that floor is absent from the list, not cleared by it), a <strong>coverage fact</strong> keyed on the tree rather than on recognizing a framework by name), and <code>joinVisibility</code> as counts (provides, consumesKeyed, consumesUnresolved) plus a meaning — no derived rate, since a quotient reads the same at 1-of-1 as at 400-of-440. <strong>Deliberately no single score</strong>: an axis zzop never measured on your tree rides in an unmeasured FIELD rather than being folded into a number that would get quoted without it. Backs <code>zzop coverage</code>, which has no MCP tool twin.`,
    },

    h324: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>validate_envelope_only_json</code>`,
      en: `<code>validate_envelope_only_json</code>`,
    },

    p25: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(envelope_json: &amp;str) -&gt; String</code>`,
      en: `<code>(envelope_json: &amp;str) -&gt; String</code>`,
    },

    p26: {
      ko: `엔벨로프 JSON &rarr; <code>{ valid: boolean, issues: string[], hints: string[] }</code> — <code>hints</code> 는 언제나 있고, 구조적으로 <strong>유효한</strong> 엔벨로프가 그럼에도 아무것도 조인하지 못할 이유를 듣는 자리다. <code>analyze_envelope_json</code> 이 자기 엔벨로프에 적용하는 구조·의미 검사를 똑같이 돌리고 거기서 멈춘다 — 설정도, 팩 로딩도, 엔진 실행도 없다 — 어댑터 작성자를 위한 빠르고 오프라인인 "내 엔벨로프가 잘 생겼나" 피드백이다. 실패하지 않는다: 유효하지 않은 엔벨로프도 평범한 <code>{ valid: false, issues: [...] }</code> 결과로 돌아온다. <code>zzop validate-envelope</code> 와 <code>validate_envelope</code> MCP 도구를 떠받친다.`,
      en: `Envelope JSON &rarr; <code>{ valid: boolean, issues: string[], hints: string[] }</code> — <code>hints</code> is always present, and is where a structurally VALID envelope is told why it will still join nothing. Runs the same structural/semantic checks <code>analyze_envelope_json</code> applies to its envelope and stops there — no config, no pack loading, no engine run — fast, offline "is my envelope well-formed" feedback for adapter authors. Never fails: an invalid envelope still returns an ordinary <code>{ valid: false, issues: [...] }</code> result. Backs <code>zzop validate-envelope</code> and the <code>validate_envelope</code> MCP tool.`,
    },

    h327: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>validate_rule_pack_json</code>`,
      en: `<code>validate_rule_pack_json</code>`,
    },

    p28: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(pack_json: &amp;str) -&gt; String</code>`,
      en: `<code>(pack_json: &amp;str) -&gt; String</code>`,
    },

    p29: {
      ko: `룰팩 JSON &rarr; <code>{ valid: boolean, issues: string[] }</code>. 엔진의 팩 로더가 적재 시점에 적용하는 구조 판정을 그대로 돌리고(망가진 JSON, 빠진 필드, 틀린 타입, 너무 새로운 <code>schema_version</code>), 여기에 적재는 되지만 말없이 영영 발화하지 못할 룰까지 잡는다 — 컴파일되지 않는 matcher 정규식, <code>line_pattern</code> 도 <code>any</code> 도 선언하지 않은 line-scan, <code>trigger</code> 가 어떤 <code>patterns</code> 항목도 선언하지 않은 라벨을 가리키는 method-scan. 모양만 보고 룰 품질의 의미는 절대 보지 않는다. 팩 작성자를 위한 배포 전 피드백이다. 실패하지 않는다: 유효하지 않은 팩도 평범한 <code>{ valid: false, issues: [...] }</code> 결과로 돌아온다. <code>zzop validate-rule-pack</code> 과 <code>validate_rule_pack</code> MCP 도구를 떠받친다.`,
      en: `Rule-pack JSON &rarr; <code>{ valid: boolean, issues: string[] }</code>. Runs the exact structural judgments the engine's pack loader applies at load time (bad JSON, missing field, wrong type, too-new <code>schema_version</code>) plus every rule that would load but could silently never fire — a matcher regex that fails to compile, a line-scan declaring neither <code>line_pattern</code> nor <code>any</code>, and a method-scan whose <code>trigger</code> names a label no <code>patterns</code> entry declares — shape only, never rule-quality semantics. Pre-ship feedback for pack authors. Never fails: an invalid pack still returns an ordinary <code>{ valid: false, issues: [...] }</code> result. Backs <code>zzop validate-rule-pack</code> and the <code>validate_rule_pack</code> MCP tool.`,
    },

    h330: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>version</code>`,
      en: `<code>version</code>`,
    },

    p31: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>() -&gt; String</code>`,
      en: `<code>() -&gt; String</code>`,
    },

    p32: {
      ko: `핑거프린트 없는 맨 릴리스 번호 — 그냥 <code>zzop version</code> / <code>zzop-mcp version</code> 이 찍는 것. <code>version_string</code> 과 일부러 갈라 뒀다: 이쪽은 스크립트가 파싱하는 토큰 하나여서, 길어지면 그렇게 하는 호출자가 전부 깨진다. MCP 도구 짝은 없고, 이유는 <code>version_string</code> 과 같다.`,
      en: `The bare release number, no fingerprints — what plain <code>zzop version</code> / <code>zzop-mcp version</code> print. Split from <code>version_string</code> deliberately: this one is a single token scripts parse, so lengthening it would break every caller that does. No MCP tool twin, for the same reason as <code>version_string</code>.`,
    },

    h333: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>version_string</code>`,
      en: `<code>version_string</code>`,
    },

    p34: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>() -&gt; String</code>`,
      en: `<code>() -&gt; String</code>`,
    },

    p35: {
      ko: `엔진 + 파서 핑거프린트 버전 문자열. <code>Result</code> 가 없다 — 실패할 수 없다. 사용자 표면에는 <code>zzop manifest</code> 와 <code>zzop facts</code> 의 <code>tool</code> 필드로, <code>zzop graph</code> 의 <code>%% tool:</code> 센서스 줄로, 그리고 <code>zzop version --verbose</code> / <code>zzop-mcp version --verbose</code> 가 찍는 것으로 닿는다. 그냥 <code>zzop version</code> 은 맨 릴리스 번호로 남고 핑거프린트를 싣지 않는다. MCP 도구 짝은 없다: 위에 이름 댄 <code>zzop-mcp version --verbose</code> 는 그 바이너리의 서브커맨드이지 MCP 호스트가 호출할 수 있는 도구가 아니다.`,
      en: `Engine + parser fingerprint version string. Has no <code>Result</code> — cannot fail. Reaches a user surface as the <code>tool</code> field of <code>zzop manifest</code> and <code>zzop facts</code>, as <code>zzop graph</code>'s <code>%% tool:</code> census line, and as what <code>zzop version --verbose</code> / <code>zzop-mcp version --verbose</code> print; plain <code>zzop version</code> stays the bare release number and carries no fingerprints. No MCP tool twin: the <code>zzop-mcp version --verbose</code> named above is a subcommand of that binary, not a tool an MCP host can call.`,
    },

    h336: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>explain</code>`,
      en: `<code>explain</code>`,
    },

    p37: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(query: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(query: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p38: {
      ko: `룰 id 하나 &rarr; 그 번들 DSL 룰의 컴파일된 데이터를 사람이 읽는 줄로. 실행에서 읽는 것은 없다 &mdash; 팩 데이터는 바이너리 <strong>안에</strong> 컴파일돼 있다. <code>Err</code> 에는 안내가 붙는다: 그 id 가 실제로 무엇인지 이름을 대고(네이티브 분석 id, 팩 전체 id, 출력 필드 id, 모호한 맨 id, 아니면 미상), 미상인 id 에 대해서는 더 넓은 코퍼스로 <code>explain_with_config</code> 를 가리킨다. MCP 도구 짝이 없는 <code>zzop explain</code> 을 떠받친다 &mdash; 에이전트는 대신 <code>rule-catalog</code> 계약 리소스를 읽는다.`,
      en: `One rule id &rarr; that bundled DSL rule&rsquo;s compiled-in data as human-readable lines. Reads nothing from a run &mdash; the pack data is compiled INTO the binary. <code>Err</code> is guided: it names what the id actually is (a native analysis id, a whole pack id, an output field id, an ambiguous bare id, or unknown), and for an unknown id it names <code>explain_with_config</code> as the wider corpus. Backs <code>zzop explain</code>, which has no MCP tool twin &mdash; an agent reads the <code>rule-catalog</code> contract resource instead.`,
    },

    h339: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>explain_with_config</code>`,
      en: `<code>explain_with_config</code>`,
    },

    p40: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `<code>(config_path: &amp;str, query: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
      en: `<code>(config_path: &amp;str, query: &amp;str) -&gt; Result&lt;String, String&gt;</code>`,
    },

    p41: {
      ko: `더 넓은 코퍼스 위의 같은 조회: 그 설정의 트리들이 실제로 적재하는 팩 &mdash; 컴파일된 것에 더해 그 트리들이 이름 대는 모든 <code>zzop/rules/</code> 와 <code>packs.extraDirs</code> 디렉터리. 번들 집합을 <strong>떠난</strong> 룰에 닿는 유일한 표면이다: 트리로 회수된 그런 룰은 자기 id 로 돌면서 발견을 내는데, 컴파일된 것만 보는 조회는 그 id 를 미상이라 부른다. 설정 파일(과 그 팩 디렉터리)을 읽는다 &mdash; 무엇도 분석하지 않는다. <code>zzop explain &lt;rule-id&gt; --config &lt;path&gt;</code> 를 떠받치고, 그냥 <code>zzop explain</code> 과 마찬가지로 MCP 도구 짝은 없다.`,
      en: `The same lookup over a wider corpus: the packs that config&rsquo;s trees actually load &mdash; the compiled-in ones plus every <code>zzop/rules/</code> and <code>packs.extraDirs</code> directory those trees name. This is the only surface that reaches a rule which LEFT the bundled set: recovered into a tree, such a rule runs and reports findings under its id while the compiled-in-only lookup calls that id unknown. Reads the config file (and its pack directories) &mdash; it does not analyze anything. Backs <code>zzop explain &lt;rule-id&gt; --config &lt;path&gt;</code>, which like plain <code>zzop explain</code> has no MCP tool twin.`,
    },

    h242: {
      ko: `설정은 필수다; 시작용 설정만으로도 전체 분석이 돈다`,
      en: `A config is required; a starter config runs the full analysis`,
    },

    p43: {
      ko: `모든 분석 레인은 <code>zzop.config.jsonc</code> 가 없는 트리를 거부한다 — 두 배포 바이너리 모두에서, 같은 거부를 같은 문서 이름을 대며 한다. 의례가 아니다: <code>vocabulary</code> 블록은 그것이 없으면 zzop 이 당신 프로젝트에 대해 추측해야 할 이름들을 담고 있고, 선언되지 않은 키는 zzop 이 <strong>내리지 않는</strong> 판정이다 — 이름 기반 인증 면제, 생성 파일 탐지, 쓰기 지점 판정이 그것과 함께 전부 꺼진다. <code>zzop init</code> 은 zzop 자신의 값이 이미 들어 있는 그 파일을 써 준다.`,
      en: `Every analysis lane refuses a tree with no <code>zzop.config.jsonc</code> — the same refusal, naming the same document, on both shipped binaries. It is not ceremony: the <code>vocabulary</code> block holds the names zzop would otherwise have to guess about your project, and an undeclared key is a judgment zzop does not make — name-based auth exemptions, generated-file detection and write-site judgments all switch off with it. <code>zzop init</code> writes that file with zzop's own values already in it.`,
    },

    p44: {
      ko: `나머지는 전부 기본값이 선다. <code>roots</code> 만 선언한 시작용 설정으로도 <em>두 바이너리 모두에서 똑같이</em> 전체 분석이 돈다 — 둘 다 공유 <code>zzop-summary</code> 층과 그 <code>zzop-config</code> 크레이트를 지나가고, 그 층이 번들 DSL 룰팩(바이너리에 컴파일돼 있다), 엔진의 <code>recentDays: 30</code> git 기본값, 그리고 <code>.zzop/cache</code> 라는 <code>cacheDir</code> 을 주입한다:`,
      en: `Everything else still defaults. A starter config that declares only <code>roots</code> runs the full analysis <em>on both binaries alike</em> — both route through the shared <code>zzop-summary</code> layer and its <code>zzop-config</code> crate, which injects the bundled DSL rule packs (compiled into the binary), the engine's <code>recentDays: 30</code> git default, and a <code>cacheDir</code> of <code>.zzop/cache</code>:`,
    },

    p45: {
      ko: `시작용 설정이 줄 수 없는 단 하나는 두 번째 트리다: 루트가 하나면 크로스레이어 조인은 이을 것이 없다. <code>pnpm-workspace.yaml</code>(또는 <code>package.json</code> 의 <code>workspaces</code>)이 2개 이상의 패키지로 풀리는 워크스페이스 루트에서는, 실행의 첫 <code>configWarnings</code> 항목이 그 매니페스트와 정확한 패키지 개수, 그리고 <code>{"trees": "auto"}</code> 라는 처방을 이름 댄다.`,
      en: `The one thing a starter config cannot supply is a second tree: a single root leaves the cross-layer join nothing to join. On a workspace root whose <code>pnpm-workspace.yaml</code> (or <code>package.json</code> <code>workspaces</code>) resolves to 2+ packages, the run's first <code>configWarnings</code> entry names the manifest, the exact package count, and the <code>{"trees": "auto"}</code> remedy.`,
    },

    p46: {
      ko: `<code>zzop_facade::analyze_json</code> 을 직접 부르면 그 앞에 그런 래퍼가 없다 — 파사드는 <strong>암묵적 기본값을 하나도</strong> 적용하지 않는다. 맨 <code>{"root": "."}</code> 설정 JSON 은 DSL 팩을 0개 적재하고(네이티브 분석만) git 수집을 끈 채 돈다(<code>scores</code>/<code>health</code>/<code>recommendations</code> 가 <code>null</code> 로 남는다) — <code>packsDir</code>/<code>packDefs</code> 와 <code>git</code> 을 직접 세우지 않는 한:`,
      en: `Calling <code>zzop_facade::analyze_json</code> directly has no such wrapper in front of it — the facade applies <strong>no implicit defaults</strong>. A bare <code>{"root": "."}</code> config JSON loads zero DSL packs (native analyses only) and runs with git collection off (<code>scores</code>/<code>health</code>/<code>recommendations</code> stay <code>null</code>) unless you set <code>packsDir</code>/<code>packDefs</code> and <code>git</code> yourself:`,
    },

    li47: {
      ko: `<strong>packsDir 생략</strong>(파사드) — DSL 팩이 아예 적재되지 않는다. <code>packsDir</code>(<code>*.json</code> 팩들이 든 디렉터리)이나 <code>packDefs</code>(인라인 정의 — 아래 참고)를 명시적으로 넘겨라.`,
      en: `<strong>packsDir omitted</strong> (facade) — no DSL packs load at all; pass <code>packsDir</code> (a directory of <code>*.json</code> packs) or <code>packDefs</code> (inline definitions — see below) explicitly.`,
    },

    li48: {
      ko: `<strong>packsDir 지정, 디렉터리 여럿</strong> — 전부 적재해 병합한다. 두 디렉터리가 같은 id 의 팩을 실으면 뒤쪽 디렉터리가 그 팩을 통째로 대체한다(룰 단위 병합이 아니다).`,
      en: `<strong>packsDir given, multiple directories</strong> — all are loaded and merged; if two directories ship a pack with the same id, the later directory replaces that pack whole (not a rule-by-rule merge).`,
    },

    li49: {
      ko: `<strong>packDefs 지정</strong> — 어떤 <code>packsDir</code> 항목보다 먼저 적재되므로, 같은 id 의 디렉터리 팩이 충돌을 통째로 이긴다. 디스크에 팩 디렉터리가 없는 호스트(예: <code>zzop-mcp</code> — 그 <code>zzop-config</code> 층이 번들 팩을 이 방식으로 주입한다)가 룰을 갖게 되는 유일한 길이 이것이다.`,
      en: `<strong>packDefs given</strong> — loaded before any <code>packsDir</code> entries, so a directory pack with the same id wins the collision whole. This is how a host with no pack directory on disk (like <code>zzop-mcp</code>, whose <code>zzop-config</code> layer injects the bundled packs this way) supplies rules at all.`,
    },

    li50: {
      ko: `<strong>git 생략</strong>(파사드) — git 수집이 꺼진다. <code>scores</code>, <code>health</code>, <code>recommendations</code>, <code>critical</code>, <code>seams</code>, <code>layerCoChurn</code> 이 <code>null</code> 로 남는다. 엔진 자신의 <code>recentDays: 30</code> 기본값으로 켜려면 <code>git: {}</code> 를, 그 값을 덮으려면 <code>git: { "recentDays": N }</code> 을 넘겨라. <code>root</code> 가 git 레포지토리가 아니면 엔진은 곱게 성능을 낮추고 그 사실을 <code>warnings</code> 에 보고한다.`,
      en: `<strong>git omitted</strong> (facade) — git collection is off; <code>scores</code>, <code>health</code>, <code>recommendations</code>, <code>critical</code>, <code>seams</code>, and <code>layerCoChurn</code> stay <code>null</code>. Pass <code>git: {}</code> to enable them with the engine's own <code>recentDays: 30</code> default, or <code>git: { "recentDays": N }</code> to override it. If <code>root</code> is not a git repository, the engine degrades gracefully and reports it in <code>warnings</code>.`,
    },

    li51: {
      ko: `<strong>cacheDir 생략</strong> — 디스크에 쓰는 유일한 기본값이고, 그래서 지금 어느 방언에 있는지 알아 둘 값어치가 있는 유일한 기본값이다. <code>zzop</code>/<code>zzop-mcp</code>/설정 파일 아래에서는 <code>.zzop/cache</code> 로 기본값이 서고, 설정 파일의 디렉터리(설정 파일이 없으면 분석 대상 루트)를 기준으로 풀리며, <em>첫 실행이 당신이 분석한 트리 안에 그것을 만든다</em> — 그 레포의 <code>.gitignore</code> 에는 <code>zzop*</code> 글롭이 아니라 앵커된 <code>**/.zzop/</code> 를 넣어라. 전자는 버전 관리에 있어야 할, 사람이 쓴 <code>zzop/</code> 디렉터리(커스텀 룰팩, 어댑터 오버레이)까지 삼킨다. 키를 <code>null</code> 로 두면 캐시가 꺼지고 아무것도 쓰지 않는다. 파사드를 직접 부르면 기본값이 아예 주입되지 않는다: 필드를 생략하면 캐시 없이 돌고 쓰는 것이 없다.`,
      en: `<strong>cacheDir omitted</strong> — the one default that writes to disk, and so the one worth knowing which dialect you are in. Under <code>zzop</code>/<code>zzop-mcp</code>/a config file it defaults to <code>.zzop/cache</code>, resolved against the config file's directory (or the analyzed root when there is no config file), and the <em>first run creates it inside the tree you analyzed</em> — put an anchored <code>**/.zzop/</code> in that repo's <code>.gitignore</code>, not a <code>zzop*</code> glob, which would also swallow the authored <code>zzop/</code> directory (custom rule packs, adapter overlays) that belongs in version control. Set the key to <code>null</code> to turn caching off and write nothing. Calling the facade directly injects no default at all: omit the field and the run is uncached, with nothing written.`,
    },

    li52: {
      ko: `<strong>vocabulary 생략</strong> — 주입되는 것도 없고 폴백되는 것도 없다 — <strong>모든</strong> 경로에서, 당신이 선언하지 않은 키는 그냥 <strong>판정되지 않는다</strong>(내장 폴백 갈래는 2026-07-27 에 제거됐다). 내장 값들은 <code>zzop init</code> 이 당신의 시작용 파일에 써 주는 것으로만 살아남는다 — 즉 그것들이 실행에 닿는 이유는 당신의 설정이 그렇게 말하기 때문이다. 그래서 이 블록을 쓰는 것은 아주 많은 것을 바꾼다: 빼 두면 룰은 <em>덜</em>이 아니라 <em>더</em> 발화한다. 선언되지 않은 가드 어휘는 어떤 가드도 증명하지 못하고, 선언되지 않은 면제는 어떤 면제도 주지 못하기 때문이다.`,
      en: `<strong>vocabulary omitted</strong> — nothing is injected and nothing falls back — on <strong>every</strong> path, a key you do not declare is simply <strong>not judged</strong> (the built-in fallback arm was removed 2026-07-27). The built-in values survive only as what <code>zzop init</code> writes into your starter file, so they reach a run because your config says them. Writing the block therefore changes a great deal: leaving it out makes rules fire <em>more</em>, not less, because an undeclared guard vocabulary proves no guard and an undeclared exemption grants no exemption.`,
    },

    h253: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `AnalyzeRequest`,
      en: `AnalyzeRequest`,
    },

    p54: {
      ko: `<code>#[serde(rename_all = "camelCase", default)]</code> — <code>root</code> 를 빼면 모든 필드가 선택이고, 모르는 필드는 거부가 아니라 무시된다.`,
      en: `<code>#[serde(rename_all = "camelCase", default)]</code> — every field is optional except <code>root</code>, and unknown fields are ignored rather than rejected.`,
    },

    summary55: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>packsDir</code><span class="cmd-row__type">string | string[] &mdash; 선택</span></span><span class="cmd-row__desc">적재할 <code>*.json</code> DSL 룰팩 디렉터리(또는 디렉터리들).<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>packsDir</code><span class="cmd-row__type">string | string[] &mdash; optional</span></span><span class="cmd-row__desc">Directory (or directories) of <code>*.json</code> DSL rule packs to load.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p56: {
      ko: `디렉터리 여럿은 적재해 병합한다 — 여러 디렉터리에 걸쳐 되풀이된 팩 id 는 뒤쪽 디렉터리 것을 통째로 취한다. 없거나 읽을 수 없는 디렉터리는 치명적이지 않은 <code>warnings</code> 항목이지 실패가 아니다.`,
      en: `Multiple directories are loaded and merged — a pack id repeated across directories is taken whole from the later directory. A missing/unreadable directory is a non-fatal <code>warnings</code> entry, not a failure.`,
    },

    summary57: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>packDefs</code><span class="cmd-row__type">object[] &mdash; 기본값 <code>[]</code></span></span><span class="cmd-row__desc">엔진에 데이터로 건네는 인라인 룰팩 정의.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>packDefs</code><span class="cmd-row__type">object[] &mdash; default <code>[]</code></span></span><span class="cmd-row__desc">Inline rule-pack definitions handed to the engine as data.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p58: {
      ko: `디스크에 팩 디렉터리가 없는 호스트를 위한, <code>packsDir</code> 의 자기완결 바이너리 대안이다(예: <code>zzop-mcp</code> 의 컴파일 시점 내장 팩). <code>packsDir</code> 디렉터리보다 <em>먼저</em> 적재되므로 같은 id 의 디렉터리 팩이 충돌을 통째로 이긴다. 더하기만 한 변경이고(v0.16.0), 은퇴한 JS 래퍼는 이것을 보낸 적이 없다. <code>analyzeEnvelope</code> 의 설정에서도 동일한 계약으로 받는다.`,
      en: `The self-contained-binary alternative to <code>packsDir</code> for hosts with no pack directory on disk (e.g. <code>zzop-mcp</code>'s compile-time-embedded packs). Loaded <em>before</em> <code>packsDir</code> directories, so a directory pack with the same id wins the collision whole. Additive (v0.16.0); the retired JS wrapper never sent it. Also accepted on <code>analyzeEnvelope</code>'s config with the identical contract.`,
    },

    summary59: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>packsOnly</code><span class="cmd-row__type">string[] &mdash; 기본값 <code>[]</code></span></span><span class="cmd-row__desc">DSL 팩 <strong>허용 목록</strong>. 설정 파일 방언: <code>packs.only</code>.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>packsOnly</code><span class="cmd-row__type">string[] &mdash; default <code>[]</code></span></span><span class="cmd-row__desc">DSL pack <strong>allowlist</strong>. Config-file dialect: <code>packs.only</code>.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p60: {
      ko: `비어 있지 않으면, id 가 여기 없는 팩은 돌지 않는다. "이것만 빼고"밖에 말하지 못하는 <code>disabledRules</code> 의 옵트인 쌍둥이다. 비어 있다는 것은 허용 목록이 <em>없다</em>는 뜻(적재된 모든 팩이 돈다)이지 아무것도 허용하지 않는다는 뜻이 아니다. 범위는 팩까지다: 네이티브 분석은 계속 돌고 <code>disabledRules</code> 의 소관으로 남는다. <code>disabledRules</code> 와 함께 쓰인다(허용 목록이 고르고, <code>disabledRules</code> 가 여전히 빼낸다).`,
      en: `When non-empty, a pack whose id is absent does not run. The opt-in twin of <code>disabledRules</code>, which can only say "everything except". Empty means <em>no</em> allowlist (every loaded pack runs), never allow-nothing. Scoped to packs: native analyses keep running and stay <code>disabledRules</code>' business. Composes with <code>disabledRules</code> (the allowlist selects, <code>disabledRules</code> still subtracts).`,
    },

    summary61: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>suppressions</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">룰별 발견 수용 목록. 경로 부분 문자열이나 글롭으로.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>suppressions</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Finding accept-list, per rule, by path substring or glob.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p62: {
      ko: `<code>{ rule, path?, glob? }</code> 하나하나가 <code>rule</code> 의 발견을 버린다 — 어디서나(필터 없음), 경로에 <code>path</code> 가 든 파일에서(부분 문자열), 또는 <code>glob</code> 에 맞는 파일에서(전체 경로 글롭. <code>glob</code> 이 <code>path</code> 를 이긴다).`,
      en: `Each <code>{ rule, path?, glob? }</code> drops findings for <code>rule</code> — everywhere (no filter), in files whose path contains <code>path</code> (substring), or in files matching <code>glob</code> (full-path glob; <code>glob</code> wins over <code>path</code>).`,
    },

    summary63: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>globalExcludes</code><span class="cmd-row__type">object[] &mdash; 기본값 <code>[]</code></span></span><span class="cmd-row__desc">설정 전체에 걸친, 룰을 가리지 않는 보고 필터 — 최상위 <code>"exclude"</code> 키.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>globalExcludes</code><span class="cmd-row__type">object[] &mdash; default <code>[]</code></span></span><span class="cmd-row__desc">Config-wide, rule-agnostic report filter — the top-level <code>"exclude"</code> key.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p64: {
      ko: `<code>suppressions</code> 와 같은 <code>path</code>/<code>glob</code> 매칭이지만, 맞는 경로를 <em>모든</em> 룰에서 한꺼번에, 그리고 다른 모든 보고 채널에서도 버린다 — <code>recommendations</code>, <code>crossLayerFindings</code>, <code>critical</code>, 그리고 <code>scores.*</code> 아래의 모든 지표별 위반 목록.`,
      en: `Same <code>path</code>/<code>glob</code> matching as <code>suppressions</code>, but drops matching paths from <em>every</em> rule at once and from every other reporting channel — <code>recommendations</code>, <code>crossLayerFindings</code>, <code>critical</code>, and every per-metric violation list under <code>scores.*</code>.`,
    },

    p65: {
      ko: `0.27 부터는 그 파일을 <strong>채점</strong>에서도 뺀다: 제외된 파일은 판정 대상이기를 그만두므로, 파일 단위 점수 뒤의 위반 목록과 분모가 함께 사라지고 <code>health.pain</code> 도 그에 따라 움직인다. 그래프 자체는 건드리지 않는다 — 그 파일은 여전히 분석되고, 여전히 의존 그래프에 있고, 여전히 실재하는 import 대상이므로 <em>다른</em> 파일의 결합도·팬아웃·폭발 반경은 바뀌지 않는다. 방향이 예측 가능하지 않다는 점에 주의하라: 평균보다 깨끗한 코드를 제외하면 pain 은 <em>올라간다</em>. 그 수치는 트리 전체가 아니라 이 설정이 판정하는 모집단을 서술하고, 같은 <code>exclude</code> 를 쓴 실행끼리만 비교 가능하기 때문이다. <code>exclude</code> 가 파일을 하나라도 걷어낸 실행은 그 사실을 <code>warnings</code> 에 적는다.`,
      en: `It also removes the file from SCORING, as of 0.27: an excluded file stops being a judged subject, leaving both the violation list and the denominator behind every per-file score, so <code>health.pain</code> moves with it. The graph itself is untouched — the file is still analyzed, still in the dep graph, and still a real import target, so no <em>other</em> file's coupling, fan-out or blast radius changes. Note the direction is not predictable: excluding code that is cleaner than average <em>raises</em> pain, because the figure describes the population this config judges rather than the whole tree, and is only comparable against runs using the same <code>exclude</code>. A run whose <code>exclude</code> removed at least one file says so in <code>warnings</code>.`,
    },

    p66: {
      ko: `필터를 아예 받지 않는 채널이 둘 있다: <code>warnings</code> 자신(문제가 <em>없어 보이게</em>만 만들 만큼 넓은 <code>exclude</code> 를 보고하는 곳 — 여기를 거르면 필터가 자기 경고를 지울 수 있게 된다), 그리고 파일이 아니라 슬라이스나 모듈로 키가 잡힌 행들(<code>cohesion.slices</code>, <code>sdp.violations</code>, <code>mainSequence.modules</code>, <code>modularity</code>) — 이쪽의 주어는 파일이 아니라 디렉터리다.`,
      en: `Two channels take no filter at all: <code>warnings</code> itself (which reports an <code>exclude</code> so broad the problem only <em>looks</em> absent — filtering it would let the filter erase its own warning), and rows keyed by a slice or module rather than a file (<code>cohesion.slices</code>, <code>sdp.violations</code>, <code>mainSequence.modules</code>, <code>modularity</code>), whose subject is a directory rather than a file.`,
    },

    summary67: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>cacheDir</code><span class="cmd-row__type">string &mdash; 선택</span></span><span class="cmd-row__desc">파일 단위 IR/룰 결과 캐시 디렉터리 — 생략하면 이 와이어는 캐시 없이 돈다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>cacheDir</code><span class="cmd-row__type">string &mdash; optional</span></span><span class="cmd-row__desc">Per-file IR/rule-result cache directory — omit and this wire runs uncached.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p68: {
      ko: `내용 해시 + 파서/룰셋 핑거프린트로 키를 잡는다. 생략하면 캐시 없이 돈다 — 기본값이 결코 주입되지 않는 <em>이 와이어에서의</em> 답이 그렇다는 뜻이다. <code>zzop</code>/<code>zzop-mcp</code>/<code>zzop.config.jsonc</code> 실행은 다른 방언이다: 그쪽 설정 프런트엔드는 이 키의 기본값을 <code>.zzop/cache</code> 로 세우고 첫 실행에서 그 디렉터리를 만든다 — <a href="#config-required">설정은 필수다</a> 를 보라.`,
      en: `Keyed by content hash + parser/ruleset fingerprint. Omit to run uncached — that is the answer <em>on this wire</em>, where no default is ever injected. A <code>zzop</code>/<code>zzop-mcp</code>/<code>zzop.config.jsonc</code> run is the other dialect: its config front end defaults the key to <code>.zzop/cache</code> and creates that directory on the first run — see <a href="#config-required">A config is required</a>.`,
    },

    summary69: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>git</code><span class="cmd-row__type">object &mdash; 선택</span></span><span class="cmd-row__desc">git 수집을 켠다 — 이력에서 나온 모든 키가 그 뒤에 앉아 있는 관문.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>git</code><span class="cmd-row__type">object &mdash; optional</span></span><span class="cmd-row__desc">Turns git collection on — the gate every history-derived key sits behind.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p70: {
      ko: `모양: <code>{ since?: string, recentDays?: number, commitTypePatterns?: { pattern, tag }[], commitSubjectPatterns?: { pattern, label }[] }</code>. git 에서 나오는 <code>scores</code>/<code>health</code>/<code>recommendations</code>/<code>critical</code>/<code>seams</code> 를 켠다.`,
      en: `Shape: <code>{ since?: string, recentDays?: number, commitTypePatterns?: { pattern, tag }[], commitSubjectPatterns?: { pattern, label }[] }</code>. Enables git-derived <code>scores</code>/<code>health</code>/<code>recommendations</code>/<code>critical</code>/<code>seams</code>.`,
    },

    p71: {
      ko: `<code>recentDays</code> 의 기본값은 <code>30</code> 이다. <code>commitTypePatterns</code> 는 비어 있지 않으면 기본 FIX/FEAT/REVERT/... 분류표를 통째로 대체한다.`,
      en: `<code>recentDays</code> defaults to <code>30</code>; <code>commitTypePatterns</code>, when non-empty, replaces the default FIX/FEAT/REVERT/... classifier table entirely.`,
    },

    p72: {
      ko: `<code>commitSubjectPatterns</code> 는 선언된 제목 라벨 축이고, 형제와 세 가지가 일부러 다르다: 기본 표가 <em>없고</em>(없거나 비어 있으면 아무것도 라벨링하지 않는다 — "revert"/"ticket"/"hotfix" 제목이 어떻게 생겼는지는 프로젝트마다 다른 관습이고 엔진은 그것을 추측하지 않는다), 첫 매치 승리가 <em>아니며</em>(맞는 선언 전부가 자기 <code>label</code> 을 선언 순서대로 보태고, 되풀이된 라벨은 첫 자리에 한 번만 남는다), <code>pattern</code> 은 쓴 그대로 컴파일돼 — 암묵적 <code>(?i)</code> 는 없다 — 날 제목에 맞춰 본다.`,
      en: `<code>commitSubjectPatterns</code> is the declared subject-label axis and differs from its sibling in three deliberate ways: it has <em>no</em> default table (absent or empty labels nothing at all — what a "revert"/"ticket"/"hotfix" subject looks like is a per-project convention the engine will not guess), it is <em>not</em> first-match-wins (every matching declaration contributes its <code>label</code>, in declaration order, a repeated label kept once at its first position), and the <code>pattern</code> is compiled exactly as written — no implicit <code>(?i)</code> — against the raw subject.`,
    },

    p73: {
      ko: `<code>warnings</code> 자기보고가 둘이다: 컴파일되지 않는 <code>pattern</code>(건너뛰고 아무것도 맞히지 않는다), 그리고 수집된 커밋을 하나도 맞히지 못한 선언 표. <strong>오늘 그 경고들이 이 키의 유일한 관측 가능한 효과다</strong> — 보존된 제목과 그 라벨은 엔진 내부의 커밋별 레코드에 머물고 아직 어떤 출력 채널에도 실리지 않는다. 알려진 한계: git 출력은 <code>from_utf8_lossy</code> 로 디코딩되므로, 레거시 인코딩 제목(<code>encoding</code> 헤더가 없는 커밋 객체)은 UTF-8 이 아닌 바이트가 이미 U+FFFD 로 바뀐 상태로 매칭된다 — 그 원래 글자를 적은 패턴은 맞힐 수 없고, U+FFFD 가 관측되면 무매치 경고가 그렇게 말한다.`,
      en: `Two <code>warnings</code> self-reports: a <code>pattern</code> that fails to compile (skipped, matches nothing), and a declared table that matched zero collected commits. <strong>Today those warnings are this key's only observable effect</strong> — the preserved subject and its labels stay on the engine-internal per-commit record and are not yet carried on any output channel. Known limit: git output is decoded with <code>from_utf8_lossy</code>, so a legacy-encoded subject (a commit object with no <code>encoding</code> header) is matched with each non-UTF-8 byte already replaced by U+FFFD — a pattern spelling those original characters cannot match it, and the zero-match warning says so when a U+FFFD is observed.`,
    },

    summary74: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>vocabulary</code><span class="cmd-row__type">object &mdash; 기본값 <code>{}</code></span></span><span class="cmd-row__desc"><strong>관습 어휘</strong> — <em>프로젝트</em>가 고르는 이름들을, 추측 대신 선언한다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>vocabulary</code><span class="cmd-row__type">object &mdash; default <code>{}</code></span></span><span class="cmd-row__desc"><strong>Convention vocabulary</strong> — the names a <em>project</em> picks, declared instead of guessed.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p75: {
      ko: `선택적인 관습 어휘 키들의 객체다 — 권위 있는 목록은 <code>zzop contract config-surface</code> 이고, 엔진 타입은 <code>zzop_engine::VocabularyConfig</code> 다.`,
      en: `An object of optional convention-vocabulary keys — the authoritative list is <code>zzop contract config-surface</code>, and the engine type is <code>zzop_engine::VocabularyConfig</code>.`,
    },

    p76: {
      ko: `프레임워크가 고정한 이름(<code>@GetMapping</code>, <code>router.post</code>)은 아무도 바꿔 부를 수 없으므로 내장으로 남는다. 프로젝트가 고르는 이름 — 인증 가드를 뭐라 부르는지, 어떤 URL 조각이 자기 API 를 표시하는지, Java 소스가 어디 사는지, 어느 디렉터리가 빌드 산출물을 담는지 — 은 여기서 선언할 수 있다. 그것을 내장 리터럴로 들고 있다는 것은 엔진이 추측한다는 뜻이고, 다르게 이름 붙인 모든 프로젝트를 말없이 오분류한다는 뜻이기 때문이다.`,
      en: `A name a framework fixed (<code>@GetMapping</code>, <code>router.post</code>) stays built in because nobody can rename it; a name the project chooses — what it calls its auth guards, which URL segments mark its API, where its Java sources live, which directories hold build output — is declarable here, because holding it as a built-in literal means the engine guesses and silently misclassifies every project that names it differently.`,
    },

    p77: {
      ko: `<strong>키 단위 통째 교체:</strong> 당신이 이름 댄 키는 자기 내장 목록이나 패턴을 통째로 대체하지, 원소 단위로 병합하지 않는다(<code>packs.extraDirs</code> 와 <code>git.commitTypePatterns</code> 가 말하는 것과 같은 한 출처 규칙이다). 빼 둔 키는 <strong>판정되지 않고</strong>, 선언했지만 비어 있는 값(<code>null</code>, <code>""</code>, <code>[]</code>)도 같은 뜻이다 — 내장 폴백은 없다(2026-07-27 제거. 내장 값들은 <code>zzop init</code> 이 당신 시작용 설정에 써 넣는 기본값으로만 살아남고, 그래서 실행에 닿는 이유는 당신의 설정이 그렇게 말하기 때문이다). 키를 빼 두면 룰은 <em>덜</em>이 아니라 <em>더</em> 발화한다: 선언되지 않은 면제는 어떤 면제도 주지 않고, 선언되지 않은 가드 어휘는 어떤 가드도 증명하지 않는다.`,
      en: `<strong>Per key, whole replacement:</strong> a key you name replaces its built-in list or pattern outright, never an element-wise merge (the same one-origin rule <code>packs.extraDirs</code> and <code>git.commitTypePatterns</code> state); a key you leave out is <strong>not judged</strong>, and a declared-but-empty value (<code>null</code>, <code>""</code>, <code>[]</code>) means the same — there is no built-in fallback (removed 2026-07-27; the built-in values survive only as the defaults <code>zzop init</code> writes into your starter config, so they reach a run because your config says them). Leaving a key out makes rules fire <em>more</em>, not less: an undeclared exemption grants no exemption and an undeclared guard vocabulary proves no guard.`,
    },

    p78: {
      ko: `"아무것도 선언하지 않기"가 "전부를 가드로 취급하기"로 적힐 수 없게 해 둔 것도 같은 이유다 — 빈 가드 <em>패턴</em>은 모든 이름에 맞는 정규식이 된다(판정을 끄고 싶으면 대신 <code>rules: { "&lt;id&gt;": "off" }</code> 를 써라). 컴파일되지 않는 선언 패턴은 실행을 실패시키는 대신 <strong>아무것도 맞히지 않는다</strong> — 내장으로 폴백하지 않는다. 작성자의 패턴을 우리 것으로 갈아 끼우는 일이야말로 이 어휘가 없애려는 그 추측이기 때문이다. <code>skipDirs</code> 는 워커 자신의 스킵 목록에 얹혀서, 목록 하나에 주인이 하나가 되게 한다.`,
      en: `That is also why "declare nothing" is deliberately not spellable as "treat everything as a guard" — an empty guard <em>pattern</em> would be a regex matching every name (disable a judgment with <code>rules: { "&lt;id&gt;": "off" }</code> instead). A declared pattern that will not compile <strong>matches nothing</strong> rather than failing the run — it never falls back to a built-in, because substituting our pattern for the author's is exactly the guessing this vocabulary removes. <code>skipDirs</code> lands on the walker's own skip list so one list has one owner.`,
    },

    p79: {
      ko: `이것은 <code>git.commitTypePatterns</code>/<code>git.commitSubjectPatterns</code> 와 <em>일부러</em> 같은 지붕이 아니다: 그쪽은 git 수집기를 설정하고 커밋 <em>메시지</em>에 맞춰 보지만, 여기의 모든 키는 분석 대상 코드 자신이 적는 것을 이름 댄다. <code>zzop init</code> 은 모든 키를 자기 내장 값과 함께 써 주므로, 시작용 파일이 이 가정들을 숨기는 대신 문서로 남긴다.`,
      en: `This is deliberately <em>not</em> the same roof as <code>git.commitTypePatterns</code>/<code>git.commitSubjectPatterns</code>: those configure the git collector and match commit <em>messages</em>, while every key here names something the analyzed code itself spells. <code>zzop init</code> writes every key with its built-in value, so the starter file documents these assumptions instead of hiding them.`,
    },

    summary80: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>profileRules</code><span class="cmd-row__type">boolean &mdash; 기본값 <code>false</code></span></span><span class="cmd-row__desc"><strong>룰 타이밍 계측</strong> — 출력의 <code>ruleTimings</code> 를 채운다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>profileRules</code><span class="cmd-row__type">boolean &mdash; default <code>false</code></span></span><span class="cmd-row__desc"><strong>Rule timing instrumentation</strong> — populates the output's <code>ruleTimings</code>.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p81: {
      ko: `ESLint 의 <code>TIMING=1</code> / oxlint 룰 타이밍에 해당한다. <code>true</code> 는 실행되는 각 DSL 룰과 각 전체 그래프 네이티브 분석의 시간을 잰다. <code>false</code> 는 <code>ruleTimings</code> 를 <code>null</code> 로 두고 추가 비용이 0 이다. <code>findings</code>/<code>ir</code> 를 바꾸는 일이 없고, 캐시 키에도 일부러 끼지 않는다 — 같은 트리를 프로파일하며 돈 실행과 그러지 않은 실행은 같은 분석이고 서로의 캐시 항목을 재사용한다.`,
      en: `The ESLint <code>TIMING=1</code> / oxlint rule-timing equivalent. <code>true</code> times each DSL rule and each whole-graph native analysis that runs; <code>false</code> leaves <code>ruleTimings</code> <code>null</code> at zero added cost. Never changes <code>findings</code>/<code>ir</code>, and deliberately takes no part in the cache key — a profiled and an unprofiled run of the same tree are the same analysis and reuse each other's cache entries.`,
    },

    p82: {
      ko: `여기서 <code>zzop.config.jsonc</code> 키가 <em>없는</em> 유일한 요청 필드다: 설정은 <em>프로젝트</em>에 대해 참인 것을 선언하고 커밋되지만, 타이밍 보고는 한 기계에서의 한 번의 호출에 대한 질문이다. CLI 방언: <code>analyze</code>/<code>analyze-envelope</code>/<code>cross</code> 의 <code>--profile-rules</code>. <code>EnvelopeAnalyzeRequest</code> 도 동일한 계약으로 같은 필드를 싣는다. 캐시에서 통째로 나온 파일은 자기 파일 단위 룰을 다시 돌리지 않으므로 타이밍에 기여하지 않는다는 점에 유의하라 — 그래서 <em>따뜻한</em> 실행은 전체 그래프 네이티브 분석만 보고하고, 나온 보고서는 이 사실을 밝히며 그것을 증명하는 캐시 개수를 함께 싣는다.`,
      en: `The one request field here with <em>no</em> <code>zzop.config.jsonc</code> key: a config declares what is true about the <em>project</em> and gets committed, while a timing report is a question about one invocation on one machine. CLI dialect: <code>--profile-rules</code> on <code>analyze</code>/<code>analyze-envelope</code>/<code>cross</code>. <code>EnvelopeAnalyzeRequest</code> carries the same field with the identical contract. Note a file served whole from cache never re-runs its per-file rules and contributes no timing, so a <em>warm</em> run reports only the whole-graph native analyses — the emitted report discloses this and carries the cache counts that prove it.`,
    },

    summary83: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>mountedAt</code><span class="cmd-row__type">string &mdash; 선택</span></span><span class="cmd-row__desc">트리 전체의 게이트웨이/인그레스 마운트 접두사. <code>http</code> provide 에만 붙는다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>mountedAt</code><span class="cmd-row__type">string &mdash; optional</span></span><span class="cmd-row__desc">Whole-tree gateway/ingress mount prefix, <code>http</code> provides only.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p84: {
      ko: `<code>dir: ""</code> 인 <code>mounts</code> 항목의 축약이고, 가장 마지막에 접혀 들어가므로 길이가 같은 명시적 <code>mounts</code> 항목이 동점을 이긴다. 코드에서 뽑아낸 접두사(예: NestJS 의 <code>setGlobalPrefix</code>) 위에 쌓인다.`,
      en: `Shorthand for a <code>mounts</code> entry with <code>dir: ""</code>, folded in last so an explicit equal-length <code>mounts</code> entry wins a tie. Stacks on top of any code-extracted prefix (e.g. NestJS's <code>setGlobalPrefix</code>).`,
    },

    summary85: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>mounts</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">디렉터리별 마운트 — provide 마다 가장 긴 <code>dir</code> 매치가 이긴다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>mounts</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Per-directory mounts — longest matching <code>dir</code> wins per provide.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p86: {
      ko: `모양: <code>{ dir: string, at: string }[]</code>. 배포 토폴로지의 디렉터리별 마운트다: <code>http</code> provide 의 파일 경로가 <code>dir</code> 아래에 떨어지면 그 키 앞에 <code>at</code> 을 붙인다.`,
      en: `Shape: <code>{ dir: string, at: string }[]</code>. Deployment-topology per-directory mounts: prepends <code>at</code> to an <code>http</code> provide's key when its file path falls under <code>dir</code>.`,
    },

    summary87: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>clientBase</code><span class="cmd-row__type">string &mdash; 선택</span></span><span class="cmd-row__desc"><code>mountedAt</code> 의 <em>부르는</em> 쪽 거울.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>clientBase</code><span class="cmd-row__type">string &mdash; optional</span></span><span class="cmd-row__desc">The <em>calling</em> side's mirror of <code>mountedAt</code>.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p88: {
      ko: `이 트리 자신의 바깥으로 나가는 http 호출이 달고 가는 경로 접두사다. 베이스가 파일을 건너뛴 상수에서 대입되고(<code>axios.defaults.baseURL = settings.baseApiUrl</code>) 그래서 추측하지 않는 추출기가 아무것도 읽지 못할 때를 위한 것이다. 트리의 키가 잡힌 <em>상대</em> <code>kind=http</code> consume 전부 앞에 붙고, 클라이언트별로 범위가 나뉘지 않는다 — 선언 하나가 트리 전체를 대변한다. 제공하는 쪽에서 <code>mountedAt</code> 이 그러는 것과 같다. 풀리지 않은 consume 과 절대 URL 키는 절대 건드리지 않는다.`,
      en: `The path prefix this tree's own outbound http calls carry, for when the base is assigned from a cross-file constant (<code>axios.defaults.baseURL = settings.baseApiUrl</code>) and the never-guess extractor therefore reads nothing. Prepended to every keyed <em>relative</em> <code>kind=http</code> consume of the tree, unscoped by client — a declaration speaks for the whole tree, as <code>mountedAt</code> does on the serving side. An unresolved consume and an absolute-URL key are never touched.`,
    },

    p89: {
      ko: `<code>mountedAt</code> 과 달리 겹쌓임은 <strong>말없이가 아니라 경고와 함께</strong> 일어난다: 코드에서 읽어낸 리터럴 베이스가 이미 적용돼 있었다면 선언이 여전히 이기지만 <code>warnings</code> 가 두 접두사를 모두 이름 댄다. 부르는 쪽에서 두 번째 접두사는 대개 진짜 두 번째 층이 아니라 중복이기 때문이다. 아무것도 다시 쓰지 못한 선언도 경고를 낸다.`,
      en: `Unlike <code>mountedAt</code>, stacking is <strong>warned, not silent</strong>: if a readable literal base was already applied from the code, the declaration still wins but <code>warnings</code> names both prefixes, because on the calling side a second prefix is usually a duplicate rather than a second real layer. A declaration that rewrites nothing warns too.`,
    },

    summary90: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>hosts</code><span class="cmd-row__type">string[]</span></span><span class="cmd-row__desc">이 트리가 소유한 호스트.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>hosts</code><span class="cmd-row__type">string[]</span></span><span class="cmd-row__desc">Hosts this tree owns.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p91: {
      ko: `다른 트리에서 이 호스트들 중 하나를 겨눈 절대 URL consume 은, 크로스레이어 링크 시점에 외부로 나가는 트래픽으로 세는 대신 내부의 조인 가능한 키로 다시 키가 잡힌다.`,
      en: `An absolute-URL consume from another tree targeting one of these hosts is re-keyed to an internal joinable key at cross-layer link time instead of counting as external egress.`,
    },

    summary92: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>routes</code><span class="cmd-row__type">object[] &mdash; 기본값 <code>[]</code></span></span><span class="cmd-row__desc">zzop 이 소스에서 풀지 못한 라우트를 하나 주입한다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>routes</code><span class="cmd-row__type">object[] &mdash; default <code>[]</code></span></span><span class="cmd-row__desc">Inject one route zzop could not resolve from source.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p93: {
      ko: `모양: <code>{ key: string, role?: "provide" | "consume" }[]</code>. 흔한 경우를 위한 가벼운 라우트 사실 주입이다: 리터럴이 아닌 경로, 동적 메서드, 계산된 URL.`,
      en: `Shape: <code>{ key: string, role?: "provide" | "consume" }[]</code>. Lightweight route-fact injection for the common case: a non-literal path, a dynamic verb, a computed URL.`,
    },

    p94: {
      ko: `<code>key</code> 는 <code>"METHOD PATH"</code> 인터페이스 키이고(예: <code>"GET /api/users"</code>), 추출기가 그쪽 방향에 쓰는 것과 같은 변환을 통해 정규화된다. <code>role</code> 은 그 라우트가 여기서 제공되는지(<code>provide</code>, 기본값) 여기서 호출되는지(<code>consume</code>)를 고른다. 배열 전체는 <code>http</code> provide/consume 으로 이뤄진 합성 어댑터 오버레이 하나로 펼쳐지고, 손으로 쓴 오버레이와 같은 크로스레이어 조인 경로를 지나며 합쳐진다. 망가진 <code>key</code> 는 경고와 함께 부드럽게 건너뛰지, 결코 단단한 에러가 아니다.`,
      en: `<code>key</code> is a <code>"METHOD PATH"</code> interface key (e.g. <code>"GET /api/users"</code>), normalized through the same transform the extractors use for that side; <code>role</code> picks whether the route is served here (<code>provide</code>, default) or called from here (<code>consume</code>). The whole array expands into one synthetic adapter overlay of <code>http</code> provides/consumes, composing through the same cross-layer join path as a hand-authored overlay. A malformed <code>key</code> is soft-skipped with a warning, never a hard error.`,
    },

    summary95: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>adapterOverlays</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">모드 B: 네이티브 분석 <em>위에</em> 병합되는 부분 엔벨로프.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>adapterOverlays</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Mode B: partial envelopes merged <em>on top of</em> native analysis.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p96: {
      ko: `부분 Normalized-AST 엔벨로프다(대개 파일 몇 개에 대한 <code>io</code> + 프래그먼트 채널뿐) — 프레임워크/SDK 어댑터가 파서를 다시 구현하지 않고도 엔진이 네이티브로 파싱하지 못하는 IoFact 를 보태는 방법이다. 오버레이마다 다시 검증하고, 유효하지 않으면 경고와 함께 부드럽게 건너뛴다. 전체 엔벨로프가 네이티브 분석을 <em>대체</em>하는 <code>analyzeEnvelope</code> 와 대비된다. <a href="https://github.com/eezz4/zzop/blob/main/docs/NORMALIZED_AST.md">NORMALIZED_AST.md</a> 를 보라.`,
      en: `Partial Normalized-AST envelopes (typically just <code>io</code> + fragment channels for a handful of files) — how a framework/SDK adapter adds IoFacts the engine does not parse natively, without reimplementing the parser. Each overlay is re-validated and soft-skipped with a warning if invalid. Contrast <code>analyzeEnvelope</code>, where a full envelope <em>replaces</em> native analysis. See <a href="https://github.com/eezz4/zzop/blob/main/docs/NORMALIZED_AST.md">NORMALIZED_AST.md</a>.`,
    },

    summary97: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>parsers</code><span class="cmd-row__type">object &mdash; 기본값 <code>{}</code></span></span><span class="cmd-row__desc">파서 라우팅 — 글롭에 맞는 경로를 이름 댄 언어로 강제한다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>parsers</code><span class="cmd-row__type">object &mdash; default <code>{}</code></span></span><span class="cmd-row__desc">Parser routing — force paths matching a glob to a named language.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p98: {
      ko: `모양: <code>{ globOverrides?: { glob: string, language: string }[] }</code>. 확장자 맵보다 <em>앞서</em> 순서대로 적용된다(첫 매치 승리). 확장자가 내용에 대해 거짓말을 하는 파일들을 위한 것이다: SQL 이 든 <code>.txt</code>, 실은 PHP 가 섞이지 않은 Java 인 벤더링된 <code>.inc</code>.`,
      en: `Shape: <code>{ globOverrides?: { glob: string, language: string }[] }</code>. Applied in order (first match wins) <em>ahead of</em> the extension map, for the files whose extension lies about what they contain: a <code>.txt</code> holding SQL, a vendored <code>.inc</code> that is really PHP-free Java.`,
    },

    p99: {
      ko: `이 빌드에 없는 언어를 이름 댄 항목은 실행을 실패시키는 대신 <strong>경고와 함께 건너뛴다</strong> — 모르는 언어는 설정을 쓰다 낸 실수이고, 그 실행의 다른 트리들은 여전히 정직하게 내놓을 답이 있다. <code>vocabulary</code> 와 일부러 지붕을 달리한다: 그쪽의 모든 키는 프로젝트가 자기 것을 <em>부르는</em> 이름을 대지만, 이쪽은 경로&rarr;파서 <em>대응</em>을 이름 댄다.`,
      en: `An entry naming a language this build does not have is <strong>skipped with a warning</strong> rather than failing the run — an unknown language is a config-authoring mistake, and the run's other trees still have honest answers to give. A separate roof from <code>vocabulary</code> on purpose: every key under that one names something the project <em>calls</em> its own, while this one names a path&rarr;parser <em>mapping</em>.`,
    },

    p100: {
      ko: `<code>analyzeEnvelope</code> 의 설정(<code>EnvelopeAnalyzeRequest</code>)은 더 작은 모양이다 — 위 필드들의 부분집합이고, 권위 있는 목록은 <code>EnvelopeAnalyzeRequest</code> 구조체 자신(<code>crates/facade/src/request.rs</code>)이다. <code>AnalyzeRequest</code> 와 공유하는 필드는 그쪽 짝과 똑같이 동작한다. <code>mountedAt</code>/<code>mounts</code> 는 위에서 설명한 배포 토폴로지 마운트 의미를 그대로 싣고, 엔벨로프의 <code>http</code> provide 에 균일하게, 같은 접힘 순서로 적용된다(<code>mounts</code> 항목 전부가 먼저, 트리 전체를 뜻하는 암묵적 <code>dir: ""</code> 항목인 <code>mountedAt</code> 이 마지막). 엔벨로프에는 엔진이 다시 읽을 수 있는 파일 시스템 위치가 없으므로 <code>root</code>, <code>cacheDir</code>, <code>git</code>, <code>sizeCap</code> 은 해당되지 않는다 — 소스 텍스트가 없으니 엔벨로프 모드에서 발화하는 DSL 룰은 <code>symbol-scan</code>/<code>io-scan</code> 뿐이다. 네이티브 콜그래프 BFS 룰(<code>mutating-route-no-auth</code>, <code>unsafe-read-endpoint</code>, <code>non-idempotent-write</code>)은 엔벨로프가 자기 <code>calls</code> 채널을 줄 때 추가로 돈다(파일별 호출 간선, <code>files[].calls</code> — NORMALIZED_AST.md 의 <code>calls</code> 절, 최소 <code>version&nbsp;&gt;=&nbsp;0.29.0</code>). http 라우트는 있는데 <code>calls</code> 가 없는 엔벨로프는 그 룰들을 침묵시키고, 침묵한 룰의 이름을 대며 <code>warnings</code> 에 그렇게 적는다. 배포된 <code>io-scan</code> 룰 둘(<code>http/protected-path-no-auth-evidence</code>, <code>http/dev-path-no-guard-hint</code>)은 여기서도 돌지만, 그 앵커 줄 채널은 돌지 않는다: 소스 텍스트가 없으면 읽을 줄이 없으므로, 파생된 <code>zzop-&lt;rule-id&gt;-ok</code> 억제 마커와 <code>dev-path-no-guard-hint</code> 의 <code>anchor_exclude_pattern</code> 가드 힌트 예외가 둘 다 무력해진다. 둘 다 침묵이 아니라 <strong>발화</strong> 쪽으로 실패한다 — 맞는 라우트는 등록 줄이 마커나 가드 힌트 인자를 달고 있어도 보고된다 — 그러니 검토를 마친 라우트를 통과시키려면 룰이 읽는 속성을 주입하거나(<code>protected-path-no-auth-evidence</code> 에는 <code>auth-guarded</code>) 설정에서 그 룰을 끄면 된다. 모드 B 의 <code>adapterOverlays</code> 는 영향을 받지 않는다: 그쪽은 소스 텍스트를 읽을 수 있는, 네이티브로 파싱된 트리 위에 병합되므로 두 채널이 그대로 살아 있다.`,
      en: `<code>analyzeEnvelope</code>'s config (<code>EnvelopeAnalyzeRequest</code>) is a smaller shape — a subset of the fields above, whose authoritative list is the <code>EnvelopeAnalyzeRequest</code> struct itself (<code>crates/facade/src/request.rs</code>); the fields it shares with <code>AnalyzeRequest</code> behave identically to their counterparts there. <code>mountedAt</code>/<code>mounts</code> carry the same deployment-topology mount semantics described above, applied uniformly to the envelope's <code>http</code> provides, with the same fold order (every <code>mounts</code> entry first, <code>mountedAt</code> as the implicit whole-tree <code>dir: ""</code> entry last). An envelope carries no filesystem location the engine can re-read, so <code>root</code>, <code>cacheDir</code>, <code>git</code>, and <code>sizeCap</code> don't apply — only <code>symbol-scan</code>/<code>io-scan</code> DSL rules ever fire in envelope mode, since no source text is available. The native call-graph-BFS rules (<code>mutating-route-no-auth</code>, <code>unsafe-read-endpoint</code>, <code>non-idempotent-write</code>) additionally run when the envelope supplies its <code>calls</code> channel (per-file call edges, <code>files[].calls</code> — see NORMALIZED_AST.md's <code>calls</code> section, floor <code>version&nbsp;&gt;=&nbsp;0.29.0</code>); an envelope with http routes and no <code>calls</code> keeps them silent and says so in <code>warnings</code>, naming the silent rules. The two shipped <code>io-scan</code> rules (<code>http/protected-path-no-auth-evidence</code>, <code>http/dev-path-no-guard-hint</code>) do run here, but their anchor-line channels do not: with no source text there is no line to read, so the derived <code>zzop-&lt;rule-id&gt;-ok</code> suppress marker and <code>dev-path-no-guard-hint</code>'s <code>anchor_exclude_pattern</code> guard-hint carve-out are both inert. Both fail toward FIRING rather than silence — a matching route reports even when its registration line carries a marker or a guard-hint argument — so clear a vetted route by injecting the attribute the rule reads (<code>auth-guarded</code> for <code>protected-path-no-auth-evidence</code>) or by disabling the rule in config. Mode-B <code>adapterOverlays</code> are unaffected: they merge onto a natively-parsed tree whose source text is readable, so both channels stay live there.`,
    },

    p101: {
      ko: `레퍼런스 모드 B 어댑터가 완성된 예제로 레포에 들어 있다 — <a href="https://github.com/eezz4/zzop/tree/main/examples/adapters/java-imports-adapter">채널 하나짜리 최소 어댑터</a>(빠진 <code>imports</code> 채널을 90 줄 남짓으로 채운다. 가장 낮은 진입로다)와 <a href="https://github.com/eezz4/zzop/tree/main/examples/adapters/auth-overlay-adapter">속성 주입 어댑터</a>(라우터 수준 가드를 파일 속성으로 주입한다), 둘 다 공유 <a href="https://github.com/eezz4/zzop/tree/main/examples/adapters/adapter-kit">adapter-kit</a> 위에 얹혀 있다. 각각 프레임워크 하나씩을 보여 주는 것이 아니라 계약을 보여 준다: 프레임워크별 맛은 어댑터가 실제로 채워야 하는 채널에 자리를 내주고 제거됐다.`,
      en: `Reference Mode-B adapters ship in the repo as worked examples — a <a href="https://github.com/eezz4/zzop/tree/main/examples/adapters/java-imports-adapter">minimal one-channel adapter</a> (fills a missing <code>imports</code> channel in ~90 lines, the smallest on-ramp) and an <a href="https://github.com/eezz4/zzop/tree/main/examples/adapters/auth-overlay-adapter">attribute-injection adapter</a> (injects router-level guards as file attributes), both built on the shared <a href="https://github.com/eezz4/zzop/tree/main/examples/adapters/adapter-kit">adapter-kit</a>. They demonstrate the contract rather than one framework each: framework-specific flavors were removed in favour of the channels an adapter actually has to fill.`,
    },

    p102: {
      ko: `IO 와 의존 사실 너머로, 오버레이는 <strong>일반 엔티티 속성</strong>을 실을 수 있다: 라우트·심볼·파일·경로 범위에 붙는 열린 어휘의 <code>{ target, key, value }</code> 주석이고, 룰이 키로 소비하되 엔진은 그 키가 무슨 뜻인지 끝까지 알지 못한다. 네이티브 패스가 혼자서는 볼 수 없는 횡단 사실 — 속도 제한, 검증 층, 또는 (네이티브 파서가 이제 직접 알아보는 흔한 Express 모양 바깥의 무엇이든) 미들웨어가 적용한 라우터 수준 인증 가드 — 을, 끝없이 자라는 네이티브 모델링 대신 주입으로 채우는 방법이 이것이다. 첫 소비자는 <code>mutating-route-no-auth</code> 다: 라우트의 <code>ioKey</code>(또는 미들웨어가 지키는 <code>pathScope</code> 접두사)에 <code>auth-guarded</code> 속성을 주입하면 룰이 그것을 통과시키고, 자기 네이티브 콜그래프 스캔과도, 알아본 Express 가드에 대해 네이티브 파서 자신이 내보내는 같은 속성과도 함께 어우러진다. 두 번째 소비자는 <code>cross-layer/retrying-write-no-idempotency</code> 다: 제공자 라우트에 붙은 <code>idempotency-guarded</code> 속성 — 핸들러가 <code>Idempotency-Key</code> 헤더를 읽을 때 TypeScript 파서가 네이티브로 세우거나, 다른 어떤 제공자 언어에서든 주입된다 — 이 가드가 목격되는 순간 그 발견에 거부권을 행사한다.`,
      en: `Beyond IO and dependency facts, an overlay can carry <strong>generic entity attributes</strong>: open-vocabulary <code>{ target, key, value }</code> annotations attached to a route, symbol, file, or path scope that a rule consumes by key, without the engine ever knowing what the key means. This is how a cross-cutting fact the native pass can't see on its own — a rate limit, a validation layer, or (for anything outside the common Express shapes the native parser now recognizes directly) a router-level auth guard applied by middleware — is completed by injection instead of by ever-growing native modeling. The first consumer is <code>mutating-route-no-auth</code>: inject an <code>auth-guarded</code> attribute on a route's <code>ioKey</code> (or a <code>pathScope</code> prefix a middleware guards) and the rule clears it, composing with its native call-graph scan and with the same attribute the native parser itself emits for a recognized Express guard. The second consumer is <code>cross-layer/retrying-write-no-idempotency</code>: an <code>idempotency-guarded</code> attribute on the provider route — set natively by the TypeScript parser when a handler reads the <code>Idempotency-Key</code> header, or injected for any other provider language — vetoes the finding once a guard is witnessed.`,
    },

    h2103: {
      ko: `여러 레포지토리를 함께 분석하기`,
      en: `Analyzing multiple repositories together`,
    },

    p104: {
      ko: `<code>analyze_trees_json</code>(CLI: <code>zzop cross</code>, MCP 도구: <code>cross_repo</code>)은 트리마다 <code>analyze</code> 를 한 번씩 돌린 다음, 모든 트리가 선언한 IoFact(HTTP/DB/tRPC 의 provide 와 consume)를 전부에 걸쳐 잇는다. 프런트엔드 체크아웃과 백엔드 체크아웃이 디스크에서 아무것도 공유하지 않는 완전히 별개의 git 레포지토리여도 조인된다.`,
      en: `<code>analyze_trees_json</code> (CLI: <code>zzop cross</code>; MCP tool: <code>cross_repo</code>) runs <code>analyze</code> once per tree, then joins every tree's declared IoFacts (HTTP/DB/tRPC provides and consumes) across all of them. A frontend checkout and a backend checkout can be two entirely separate git repositories that share nothing on disk and still get joined.`,
    },

    p105: {
      ko: `결과 모양은 <code>{ trees: [{ root, sourceId, output }], crossLayer, crossLayerFindings, disclosure }</code> 다. 트리마다의 <code>output</code> 은 자기 <code>coverage</code> 센서스를 싣고, <code>disclosure</code>(침묵 실패 계열 등기부)는 실행 전역이라 한 번만 나온다. <code>crossLayer</code> 는 날 조인 결과를 싣는다 — 맞은 <code>edges</code>, <code>unconsumedProvides</code>, <code>unprovidedConsumes</code>, <code>unresolvedConsumes</code>, 다중 트리 매치인 <code>ambiguousConsumes</code>, 그리고 절대 URL 인 <code>externalConsumes</code>. 어느 트리든 토폴로지 <code>hosts</code> 를 선언하면 <code>crossLayer</code> 는 <code>hostRekeyCounts</code> 도 싣는다 — 선언된 호스트마다 <code>[host, rekeyedConsumeCount]</code> 짝 하나이고, 어떤 트리도 호스트를 선언하지 않으면 통째로 빠진다. <code>crossLayerFindings</code> 는 그 조인 위에서 도는 <code>cross-layer/*</code> 네이티브 룰의 출력이다(전체 id 목록은 룰 카탈로그를 보라). 크로스레이어 발견은 어느 한 트리의 것이 아니므로, <em>어느 한</em> 트리에서 <code>disabledRules</code> 로 이 룰 id 중 하나를 끄면 그것이 모든 트리의 합친 배열에서 빠진다 — 트리별 관문이 아니라 합집합이다. <code>coverage.joinContributionZero</code> 가 <code>true</code> 인 트리는 이 조인에 IO 를 하나도 보태지 않았다 — 그 트리를 참조하는 크로스레이어 발견은 그만큼 깎아 읽어라.`,
      en: `The result shape is <code>{ trees: [{ root, sourceId, output }], crossLayer, crossLayerFindings, disclosure }</code>. Each tree's <code>output</code> carries its own <code>coverage</code> census; <code>disclosure</code> (the silent-failure-class registry) is run-global and appears once. <code>crossLayer</code> carries the raw join result — matched <code>edges</code>, <code>unconsumedProvides</code>, <code>unprovidedConsumes</code>, <code>unresolvedConsumes</code>, <code>ambiguousConsumes</code> multi-tree matches, and <code>externalConsumes</code> (absolute-URL) consumes. When any tree declares topology <code>hosts</code>, <code>crossLayer</code> also carries <code>hostRekeyCounts</code> — one <code>[host, rekeyedConsumeCount]</code> pair per declared host, omitted entirely when no tree declares any hosts. <code>crossLayerFindings</code> is the output of the <code>cross-layer/*</code> native rules that run over that join (see the rule catalog for the full id list). No single tree owns a cross-layer finding, so disabling one of these rule ids via <code>disabledRules</code> on <em>any one</em> tree drops it from the combined array for every tree — a union, not a per-tree gate. A tree whose <code>coverage.joinContributionZero</code> is <code>true</code> contributed no IO to this join — discount any cross-layer finding that references it.`,
    },

    h2106: {
      // No prose: a signature or a type name. The Korean edition prints the same characters.
      ko: `AnalyzeOutputView`,
      en: `AnalyzeOutputView`,
    },

    p107: {
      ko: `같은 입력이면 바이트까지 같은 출력이다 — 타임스탬프도, 불안정한 맵/배열 순서도 없다. 어떤 실행이 제공할 수 없는 능력은 <strong>스키마에서 빠지고 <code>warnings</code> 에 스스로 보고된다</strong>. 가짜 빈 값으로 채워 넣는 일은 결코 없다. 반대로 빈 배열은 언제나 "분석했고 아무것도 찾지 못했다"는 뜻이다.`,
      en: `Same input, byte-identical output — no timestamps, no unstable map/array ordering. A capability a given run cannot provide is <strong>absent from the schema and self-reported in <code>warnings</code></strong>, never stubbed with a fake empty value. An empty array, by contrast, always means "this was analyzed and nothing was found."`,
    },

    p108: {
      ko: `이 색인은 <em>와이어</em> 모양이고, 이름이 가리키는 타입보다 한 겹 넓다: 응답 루트는 <code>AnalyzeOutputView</code> 를 평평하게 펴고 실행 전역인 <code>disclosure</code> 등기부를 형제로 덧붙인다. 두 쪽 다 <code>docs/contracts/surface-parity.json</code> 에 한 번씩 열거돼 있다 — 최상위 키는 전부 거기에 행이 있어야 하고, 없으면 빌드 테스트가 실패한다.`,
      en: `This index is the <em>wire</em> shape, which is one layer wider than the type it is named for: the reply root flattens <code>AnalyzeOutputView</code> and adds the run-global <code>disclosure</code> registry as a sibling. Both halves are enumerated once, in <code>docs/contracts/surface-parity.json</code> — every top-level key must have a row there, and a build test fails otherwise.`,
    },

    summary109: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>coverage</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">구조 커버리지 센서스 — 언제나 있다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>coverage</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">Structural coverage census — always present.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p110: {
      ko: `이 트리가 어느 채널을 채웠는지를 어휘 없이 세어 놓은 것(<code>files</code>, <code>parserDispatched</code>, <code>symbols</code>, <code>resolvedImportEdges</code>, <code>declaredImportsByExt</code> — 그 간선 개수에 대한 확장자별 선언 지정자 분모이고 해석 이전에 센다. 확장자 키가 없다는 것은 0 이 아니라 잰 적이 없다는 뜻이다 — <code>ioProvides</code>, <code>ioConsumesKeyed</code>, <code>ioConsumesUnresolved</code>, <code>degraded</code>)에 더해, 능동적 실명 사실인 <code>joinContributionZero</code> 가 실린다. 각 칸의 정의는 아래 <a href="#coverage-census">커버리지 센서스</a>에 있다.`,
      en: `Vocab-free counts of which channels this tree filled (<code>files</code>, <code>parserDispatched</code>, <code>symbols</code>, <code>resolvedImportEdges</code>, <code>declaredImportsByExt</code> — the per-extension declared-specifier denominator for that edge count, counted before resolution; an absent extension key means never measured, not 0 — <code>ioProvides</code>, <code>ioConsumesKeyed</code>, <code>ioConsumesUnresolved</code>, <code>degraded</code>) plus the active-blindness fact <code>joinContributionZero</code>. Every cell is defined in <a href="#coverage-census">the coverage census</a> below.`,
    },

    summary111: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>folders</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc"><code>nodes</code> 와 의존 그래프를 폴더 단위로 굴려 올린 것.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>folders</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">Folder-granularity rollup of <code>nodes</code> and the dependency graph.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p112: {
      ko: `git 이 관문이 아니다 — 빈 트리에서도 언제나 있다. 각 행이 세는 것은 <code>fileCount</code> 가 아니라 <code>nodeCount</code> 다: 노드는 의존 그래프 키이거나 git 이 건드린 경로일 때만 존재하므로, 이 행들을 더해도 최상위 <code>fileCount</code>(워크한 파일 수)가 나오지 않고, git 이 꺼져 있으면 lexical-only 파일은 어느 행에도 없다.`,
      en: `Not gated by git — always present, even for an empty tree. Each row counts <code>nodeCount</code>, not <code>fileCount</code>: a node exists only for a dep-graph key or a git-touched path, so summing these rows does not reproduce the top-level <code>fileCount</code> (files walked), and with git off a lexical-only file is in no row at all.`,
    },

    summary113: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>findings</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc"><code>(severity, file, line, ruleId)</code> 오름차순 정렬, critical 이 먼저.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>findings</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Sorted <code>(severity, file, line, ruleId)</code> ascending, critical first.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p114: {
      ko: `인라인 마커 주석으로 억제된 발견은 아예 나타나지 않는다. 모든 발견이 공유하는 모양과, 그 <code>severity</code> 가 무엇을 주장하고 무엇을 주장하지 않는지는 <a href="#finding-shape">색인 아래에 정의돼</a> 있다.`,
      en: `A finding suppressed by an inline marker comment never appears at all. The shape every finding shares — and what its <code>severity</code> does and does not assert — is <a href="#finding-shape">defined below the index</a>.`,
    },

    summary115: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>scores</code><span class="cmd-row__type">object | null</span></span><span class="cmd-row__desc">구조 세부 점수, 0&ndash;100. <code>git</code> 이 서 있지 않으면 <code>null</code>.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>scores</code><span class="cmd-row__type">object | null</span></span><span class="cmd-row__desc">Structural sub-scores, 0&ndash;100. <code>null</code> unless <code>git</code> is set.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p116: {
      ko: `<strong>모든 점수는 자기가 채점한 모집단을 함께 싣는다</strong> — <code>featureSlicedDesign.layerClassifiedImports</code>, <code>cohesion.sliceCount</code>, <code>coupling.importerCount</code>, <code>godFile.total</code>, <code>busFactor.total</code>, <code>diamond.rootsExamined</code>, <code>mainSequence.classifiedFiles</code> 와 그 형제들. 모집단 <code>0</code> 은 건강 증명서가 아니라 <em>잰 적이 없다</em>는 신호다: 모든 공식이 빈 모집단에서 100 을 돌려주므로, "전부 판정했고 전부 통과했다"와 "판정할 수 있는 것을 하나도 찾지 못했다"를 가르는 것은 분모뿐이다.`,
      en: `<strong>Every score ships the population it scored over</strong> — <code>featureSlicedDesign.layerClassifiedImports</code>, <code>cohesion.sliceCount</code>, <code>coupling.importerCount</code>, <code>godFile.total</code>, <code>busFactor.total</code>, <code>diamond.rootsExamined</code>, <code>mainSequence.classifiedFiles</code> and their siblings. A population of <code>0</code> is the <em>never measured</em> signal, not a clean bill of health: every formula returns 100 on an empty population, so the denominator is the only thing separating "judged everything and all passed" from "found nothing it could judge".`,
    },

    p117: {
      ko: `각각은 판정된 모집단의 분모이지 트리 총계가 아니다: 최상위 <code>exclude</code> 는 경로를 위반 목록에서도 분모에서도 빼고, <code>fileSizeCompliance</code>/<code>godFile</code> 은 소스 파일만 판정한다. <code>mainSequence.classifiedFiles</code> 는 현재 모든 빌드에서 <code>0</code> 이므로(파일을 추상/구상으로 분류하는 것이 아직 없다) 그 <code>instability</code>/<code>fileCount</code> 만 읽어라. 각 키가 무슨 뜻인지는 <code>scoreMeanings</code> 에 나란히 실려 온다.`,
      en: `Each is a judged-population denominator, never a tree total: the top-level <code>exclude</code> removes a path from both the violation list and the denominator, and <code>fileSizeCompliance</code>/<code>godFile</code> judge source files only. <code>mainSequence.classifiedFiles</code> is <code>0</code> on every current build (nothing classifies a file abstract vs concrete), so read only its <code>instability</code>/<code>fileCount</code>. What each key means rides beside it in <code>scoreMeanings</code>.`,
    },

    summary118: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>scoreMeanings</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc"><code>scores</code> 의 키마다 한 문장씩, 키는 똑같이.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>scoreMeanings</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">One sentence per <code>scores</code> key, keyed identically.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p119: {
      ko: `<code>scores</code> 가 있을 때 정확히 함께 있고, 아니면 <code>null</code> 이 아니라 아예 없다. 점수 키 하나가 맨 약어이고 그 풀이가 사는 자리가 이 범례다: <code>sdp</code>(Stable Dependencies Principle). 둘이 더 그랬는데 그쪽은 대신 와이어에서 이름을 바꿨다: <code>sfc</code> &rarr; <code>fileSizeCompliance</code>, <code>fsd</code> &rarr; <code>featureSlicedDesign</code>. 네 번째였던 <code>lod</code>(Law of Demeter)는 2026-08 에 자기 점수와 함께 제거됐다 — 무엇도 잰 적이 없었다. 각 문장은 <em>낮은</em> 수가 무슨 뜻인지를 말한다. 모든 점수는 0&ndash;100 이고 높을수록 건강하다.`,
      en: `Present exactly when <code>scores</code> is, absent (not <code>null</code>) otherwise. One score key is a bare acronym and the legend is where it is expanded: <code>sdp</code> (Stable Dependencies Principle). Two more used to be and were renamed on the wire instead: <code>sfc</code> &rarr; <code>fileSizeCompliance</code>, <code>fsd</code> &rarr; <code>featureSlicedDesign</code>. A fourth, <code>lod</code> (Law of Demeter), was removed along with its score in 2026-08 — it had never measured anything. Each sentence says what a <em>low</em> number means; all scores are 0&ndash;100, higher is healthier.`,
    },

    summary120: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>health</code><span class="cmd-row__type">object | null</span></span><span class="cmd-row__desc"><code>scores</code> 를 굴려 올린 하나의 종합 지수 — 그리고 룰 발견은 싣지 않는다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>health</code><span class="cmd-row__type">object | null</span></span><span class="cmd-row__desc">One composite index rolled up from <code>scores</code> — and it carries no rule findings.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p121: {
      ko: `모양: <code>{pain, axisPain[], measuredWeight, totalWeight, contributors[]}</code>. <strong><code>pain</code> 은 룰 발견을 싣지 않는다</strong> &mdash; SQL 인젝션으로 가득한 트리와 하나도 없는 같은 트리의 점수가 똑같다 &mdash; 그리고 그것이 싣는 것의 대부분은 결함 주장이 아니라 구조에 대한 <em>의견</em>이다.`,
      en: `Shape: <code>{pain, axisPain[], measuredWeight, totalWeight, contributors[]}</code>. <strong><code>pain</code> carries no rule findings</strong> &mdash; a tree full of SQL injection scores exactly what the same tree scores with none &mdash; and most of what it does carry is a structural <em>opinion</em> rather than a defect claim.`,
    },

    p122: {
      ko: `<code>axisPain[]</code> 이 와이어에서 그렇게 말한다: <code>pain</code> 을 <code>defect</code>(import 순환, 유일한 항목), <code>opinion</code>(배럴 규율, FSD 계층, SDP/Main Sequence, Newman 모듈성, LOC 상한 &mdash; 일부러 반대로 하는 프로젝트는 틀린 것이 아니라 점수가 낮을 뿐이다), <code>history</code>(이름 변경 처닝, 버스 팩터)로 쪼개고, 각각을 <code>pain</code> 자신의 척도에 얹으며, 셋을 더하면 그 값이 된다.`,
      en: `<code>axisPain[]</code> says so on the wire: it splits <code>pain</code> into <code>defect</code> (import cycles, the only one), <code>opinion</code> (barrel discipline, FSD layering, SDP/Main Sequence, Newman modularity, LOC ceilings &mdash; a project that deliberately does the opposite is not wrong, it scores low) and <code>history</code> (rename churn, bus factor), each on <code>pain</code>&rsquo;s own scale and summing to it.`,
    },

    p123: {
      ko: `<code>pain</code> 은 모집단이 <em>있었던</em> 지표들 위에서 다시 정규화하므로, 이 트리가 재지 못한 축은 조용히 100 점을 받아 레포를 더 건강해 보이게 만드는 대신 가중치에서 통째로 빠진다. <code>pain</code> 은 지표 표의 얼마만큼이 여기서 잴 수 있었는지를 말하는 <code>measuredWeight / totalWeight</code> 와 함께 읽어라. 하나도 잴 수 없었으면 <code>pain</code> 은 (<code>0</code> 이 아니라) <code>null</code> 이다. <code>contributors[]</code> 는 재지 못한 지표를 <code>population: 0</code> 과 <code>null</code> 격차를 가진 행으로 남겨 두므로, 어두운 축은 사라지는 대신 그렇게 말해진다.`,
      en: `<code>pain</code> renormalizes over the metrics that <em>had</em> a population, so an axis this tree could not measure leaves the weighting entirely instead of quietly scoring 100 and making the repo look healthier. Read <code>pain</code> against <code>measuredWeight / totalWeight</code>, which says how much of the metric table was measurable here; <code>pain</code> is <code>null</code> (never <code>0</code>) when none of it was. <code>contributors[]</code> keeps the unmeasured metrics as rows with <code>population: 0</code> and a <code>null</code> gap, so a dark axis is stated rather than absent.`,
    },

    summary124: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>recommendations</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">ROI 로 순위 매긴 리팩터링 후보.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>recommendations</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">ROI-ranked refactor candidates.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p125: {
      ko: `룰이 확인한 critical 발견을 자기 파일에 달고 있는 항목은 합성 그룹 <code>urgent-bug-risk</code> 로 옮겨 간다(복사가 아니라 이동이다). 그 <code>roi</code> 수치는 바뀌지 않는다.`,
      en: `An item whose file carries a rule-confirmed critical finding moves (never copies) into a synthetic <code>urgent-bug-risk</code> group; its <code>roi</code> number never changes.`,
    },

    summary126: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>critical</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc"><em>크기로 가중한</em> 폭발 반경으로 순위 매긴 파일.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>critical</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Files ranked by <em>size-weighted</em> blast radius.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p127: {
      ko: `<code>blastRadius * ln(loc + 2)</code>, 동점일 때 폭발 반경으로 가른다 — 폭발 반경이 같은 5 줄짜리 재수출 배럴과 400 줄짜리 코어는 같은 위험이 아니기 때문이다. <code>blastRadius</code> 자체는 이행적 의존자 수이고, 이 배열을 그것만으로 다시 정렬하면 다른 순서가 나온다.`,
      en: `<code>blastRadius * ln(loc + 2)</code>, blast radius as tie-break — because a 5-line re-export barrel and a 400-line core of equal blast are not equal danger. <code>blastRadius</code> itself is the transitive dependent count; re-sorting this array by it alone gives a different order.`,
    },

    summary128: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>seams</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">가장 먼저 떼어내기 좋은 폴더 후보(경계를 넘는 결합이 적은 곳).<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>seams</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Folders that are good first-extraction candidates (low boundary-crossing coupling).<span class="cmd-row__more">contract</span></span></div>`,
    },

    p129: {
      ko: `<code>files</code> 가 세는 것은 그 폴더의 <em>의존 그래프 키</em>이지 워크한 파일이 아니고, 잡음 폴더(테스트, dist, docs, &hellip;)는 통째로 건너뛴다. <code>temporalBoundary</code> 는 두 번 걸러진다 — 파일 2&ndash;25 개를 건드린 커밋만, 그리고 파일마다 상위 10 개 동시 변경 짝만 — 그래서 그것은 총계가 아니라 <strong>측정된 것 중 가장 강한</strong> 폴더 간 동시 변경이다.`,
      en: `<code>files</code> counts that folder's <em>dep-graph keys</em>, not files walked, and noise folders (tests, dist, docs, &hellip;) are skipped whole. <code>temporalBoundary</code> is filtered twice over — only commits touching 2&ndash;25 files, and only each file's top 10 co-change partners — so it is the strongest measured cross-folder co-change, not a total.`,
    },

    summary130: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>layerCoChurn</code><span class="cmd-row__type">object[] | null</span></span><span class="cmd-row__desc">레이어를 가로지르는 커밋 동시 처닝 짝.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>layerCoChurn</code><span class="cmd-row__type">object[] | null</span></span><span class="cmd-row__desc">Cross-layer commit co-churn pairs.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p131: {
      ko: `<code>git</code> 이 서 있지 않으면 <code>null</code>. git 은 살아 있는데 동시 변경 문턱을 넘는 짝이 없으면 (<code>null</code> 이 아니라) <code>[]</code>. <code>coChanges</code> 는 <em>부분집합</em> 총계다: 파일 2 개 미만이나 25 개 초과를 건드린 커밋은 잡음으로 건너뛰므로, "거른 뒤 동시 변경 N 회"로 읽어야지 "이 레이어들이 N 번 함께 바뀌었다"로 읽으면 안 된다.`,
      en: `<code>null</code> unless <code>git</code> is set; <code>[]</code> (not <code>null</code>) when git is active but no pair meets the co-change threshold. <code>coChanges</code> is a <em>subset</em> total: commits touching fewer than 2 or more than 25 files are skipped as noise, so read it as "N filtered co-changes", never "these layers changed together N times".`,
    },

    summary132: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>coChange</code><span class="cmd-row__type">object[] | null</span></span><span class="cmd-row__desc">파일 짝 동시 변경 간선 — <code>graph --domain cochange</code> 가 그리는 바탕.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>coChange</code><span class="cmd-row__type">object[] | null</span></span><span class="cmd-row__desc">File-pair co-change edges — the substrate <code>graph --domain cochange</code> draws.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p133: {
      ko: `의존 그래프 설명이 기대는 것과 같은 증거다. <code>null</code> = git 이 비활성이거나 수집이 실패해 아무것도 재지 못했다. <code>[]</code> = 쟀고 함께 바뀐 것이 없었다 — 이 둘을 하나로 접으면 안 된다. <code>layerCoChurn</code> 과 같은 잡음 필터 둘을 그대로 달고 있으므로 총계가 아니라 표본이다. <code>disabledRules</code> 가 관문이 아니다: 이것은 룰의 판정이 아니라 측정된 증거다. 경로는 <code>nodes</code>·<code>dep</code> 와 마찬가지로 분석 대상 트리 기준이다: 이력은 레포지토리 단위로 모은 다음 각 트리 루트로 다시 얹히므로, 모노레포 안의 패키지는 자기 동시 변경만 보고 형제의 것은 결코 보지 않는다.`,
      en: `The same evidence the dep-graph descriptions draw on. <code>null</code> = git inactive or collection failed, so nothing was measured; <code>[]</code> = measured and nothing co-changed — the two must not be folded together. Carries the same two noise filters as <code>layerCoChurn</code>, so it is a sample rather than a total. Not gated by <code>disabledRules</code>: it is measured evidence, not a rule verdict. Paths are relative to the analyzed tree, like <code>nodes</code> and <code>dep</code>: history is collected per repository and then rebased onto each tree root, so a package inside a monorepo sees its own co-change and never its sibling's.`,
    },

    summary134: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>gitWindow</code><span class="cmd-row__type">object | null</span></span><span class="cmd-row__desc">해석된 git 이력 창을 되울린다 — 언제나 직렬화된다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>gitWindow</code><span class="cmd-row__type">object | null</span></span><span class="cmd-row__desc">Echoes the resolved git-history window — always serialized.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p135: {
      ko: `모양: <code>{ recentDays, since } | null</code>. <code>null</code> 은 "git 이 돌지 않았다"는 신호다(<code>scores</code> 와 같은 관문). <code>recentDays</code> 는 해석된 수(호출자의 값, 아니면 기본값 <code>30</code>)이고, <code>since</code> 는 호출자가 준 날 필터 문자열이거나 이력 전체일 때 <code>null</code> 이다.`,
      en: `Shape: <code>{ recentDays, since } | null</code>. <code>null</code> is the "git didn't run" signal (same gating as <code>scores</code>). <code>recentDays</code> is the resolved number (the caller's value, or the <code>30</code> default); <code>since</code> is the caller's raw filter string, or <code>null</code> for full history.`,
    },

    summary136: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>packsLoaded</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">팩 적재의 긍정 확인: 적재된 DSL 팩마다 한 항목. 끈 팩은 <code>didNotRun</code> 으로 갈린다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>packsLoaded</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Positive pack-load confirmation: one entry per loaded DSL pack; a pack that did not run is marked <code>didNotRun</code>.<span class="cmd-row__more">contract</span></span></div>`,
    },

    summaryPacksLoadedMeaning: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>packsLoadedMeaning</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">위 배열의 범례. 팩이 하나도 안 실렸으면 아예 없다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>packsLoadedMeaning</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">The legend for the array above; absent entirely when no pack loaded.<span class="cmd-row__more">contract</span></span></div>`,
    },

    summaryNativeAnalyses: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>nativeAnalyses</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc"><code>packsLoaded</code> 가 DSL 팩에 묻는 것을 내장 분석에 묻는다: 이 응답의 <code>findings</code> 에 키를 만들 수 <em>없었던</em> 것이 무엇인가.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>nativeAnalyses</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">The same question as <code>packsLoaded</code>, asked of the built-in analyses: which of them could not have keyed this reply's <code>findings</code>.<span class="cmd-row__more">contract</span></span></div>`,
    },

    pNativeAnalyses1: {
      ko: `모양: <code>{ registered, disabled, reportedInCrossLayerFindings }</code>. <code>registered</code> 는 이 빌드가 담은 내장 분석의 수 — 아래 두 목록을 읽을 때의 <strong>분모</strong>다. 세는 것은 <strong>게이트 id 공간</strong>(<code>rules</code>/<code>disabledRules</code> 가 부르는 이름)이지 발견이 실을 수 있는 id 가 아니다: 일부는 발견을 안 내는 점수 계산을 게이트하고, 일부는 발견이 더 잘게 <code>schema/&lt;label&gt;</code> 로 나가는 우산 id 다.`,
      en: `Shape: <code>{ registered, disabled, reportedInCrossLayerFindings }</code>. <code>registered</code> is how many native analyses this build carries — the denominator the two lists are read against. It counts the <strong>gate id space</strong> (what <code>rules</code>/<code>disabledRules</code> name), not the ids a finding can carry: some gate score computations that emit no finding, and some are umbrella ids whose findings carry finer <code>schema/&lt;label&gt;</code> names.`,
    },

    pNativeAnalyses2: {
      ko: `두 목록을 가르는 이유는 처방이 정반대라서다. <code>disabled</code> = 당신의 config 가 껐다 — <code>findings</code> 에 없다는 것은 <strong>분석 안 됨</strong>이고, 판정을 받으려면 다시 켜라. <code>reportedInCrossLayerFindings</code> = <strong>켜져 있는데도</strong> 여기 나올 수 없다. 이들은 크로스트리 조인을 판정하고 그 자신의 <code>crossLayerFindings</code> 채널로 보고하는데, 트리별 출력에는 그 채널이 없다 — 같은 config 로 크로스레이어 조인을 돌리면 보인다. 등록됐는데 어느 목록에도 없으면 그것은 돌았다 — 위 문단이 이름 댄 부류만 예외다. 점수 계산을 게이트하는 id 는 발견을 아예 안 내고, 우산 id 의 발견은 더 잘게 <code>schema/&lt;label&gt;</code> 이름으로 나온다. 어느 쪽도 <code>registered</code> 가 세는 자기 이름으로는 <code>findings</code> 에 키를 만들지 않으니, 거기가 비어 있는 것은 그 id 들의 평소 모습이지 판정이 아니다(우산이면 <code>schema/&lt;label&gt;</code> 쪽을 찾아라). 그 밖에 어느 목록에도 없는 것은 <code>findings</code> 에 없는 것이 실측된 0 이다.`,
      en: `The two lists are kept apart because the remedies are opposite. <code>disabled</code> = your config switched these off, so their absence from <code>findings</code> means <strong>not analyzed</strong>; turn one back on to get a verdict. <code>reportedInCrossLayerFindings</code> = these are switched <strong>on</strong> and still cannot appear here, because they judge the cross-tree join and report into its own <code>crossLayerFindings</code> channel, which a per-tree output does not have; run the cross-layer join over the same config to see them. Anything registered and in neither list ran — except for the kinds the paragraph above named. A score-gating id emits no finding at all, and an umbrella id's findings arrive under the finer <code>schema/&lt;label&gt;</code> names, so neither can key <code>findings</code> under the name <code>registered</code> counted: a blank there is how those ids always look, not a verdict (for an umbrella, look under <code>schema/&lt;label&gt;</code> instead). For everything else in neither list, absence from <code>findings</code> is a measured zero.`,
    },

    pNativeAnalyses3: {
      ko: `두 목록은 비어 있어도 항상 실린다(<code>[]</code> 포함) — <code>zeroAdmissionRules</code> 와 달리, 고의로. 항목이 있을 때만 나오는 목록은 깨끗한 실행에서 아무 말도 안 하고, 그 바이트는 <em>보고하지 않는 빌드</em>와 구분되지 않는다. 이 객체가 없애려는 혼동이 바로 그것이다.`,
      en: `Both lists are always serialized, <code>[]</code> included — unlike <code>zeroAdmissionRules</code>, deliberately. A list that appears only when it has entries reports nothing on a clean run in bytes indistinguishable from a build that does not report at all, which is the confusion this object exists to end.`,
    },

    summaryNativeAnalysesMeaning: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>nativeAnalysesMeaning</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">위 객체의 범례 — 주제가 항상 있으므로 범례도 항상 있다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>nativeAnalysesMeaning</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">The legend for the object above — always present, because its subject always is.<span class="cmd-row__more">contract</span></span></div>`,
    },

    pNativeAnalysesMeaning: {
      ko: `키마다 한 문장씩: <code>registered</code>, <code>disabled</code>, <code>reportedInCrossLayerFindings</code>, 그리고 <code>everythingElse</code> — 자기 필드가 없는 잔여 부류인데, 없는 것이 <em>곧</em> 판정인 경우가 정확히 그것이라서 그렇다. <code>packsLoadedMeaning</code> 과 달리 무조건 실린다: 팩을 하나도 안 싣는 실행은 정당하게 있지만, 내장 분석을 등록하지 않는 빌드는 없다.`,
      en: `One sentence per key: <code>registered</code>, <code>disabled</code>, <code>reportedInCrossLayerFindings</code>, plus <code>everythingElse</code> — the residual class that has no field of its own precisely because it is the case where absence <em>is</em> the verdict. Unconditional, unlike <code>packsLoadedMeaning</code>: a run can legitimately load no pack, but every build registers native analyses.`,
    },

    pPacksLoadedMeaning: {
      ko: `키마다 한 문장씩: <code>row</code>(한 항목 = <strong>적재된</strong> 팩 하나 — 적재는 실행이 아니다), <code>filesInScope</code>(경로 후보 수이지 “맞았다”가 아니고, <strong>이 키가 있다는 것 자체가 그 팩이 돌았다는 주장</strong>이다), <code>zeroAdmissionRules</code>(그 룰들에는 <strong>분석 파일이 한 개도 안 들어왔다</strong> — “파일은 들어왔는데 안 터졌다”가 <em>아니다</em>. 함의가 정반대인 두 읽기 중 언제나 앞쪽이었고, 이제 그렇게 적는다). 네 번째 <code>didNotRun</code> 은 <strong>실제로 꺼진 팩이 있을 때만</strong> 나온다 — 해당 없는 상태를 설명하는 범례는 소음이고, 소음이 독자에게 공시를 건너뛰게 만든다. 배열 안이 아니라 옆에 두는 이유는 <code>packsLoaded</code> 가 배열이라서다: 안에 넣으면 팩 수만큼 같은 문장이 반복된다(<code>scoreMeanings</code> 와 같은 모양).`,
      en: `One sentence per key: <code>row</code> (an entry is one <strong>loaded</strong> pack — loading is not running), <code>filesInScope</code> (path candidacy, never a "matched" count, and <strong>the presence of the key is itself the claim that the pack ran</strong>), and <code>zeroAdmissionRules</code> (those rules admitted <strong>no analyzed file at all</strong> — <em>not</em> "files reached them and nothing fired"). Those two readings imply opposite things; it has always meant the first, and now says so. A fourth key, <code>didNotRun</code>, appears <strong>only when a pack really was gated off</strong> — a legend entry for a state this run is not in is noise, and noise is what teaches readers to skip disclosures. It sits beside the array rather than inside it because <code>packsLoaded</code> is an array: inside, the same sentences would repeat once per loaded pack (same shape as <code>scoreMeanings</code>).`,
    },

    p137: {
      ko: `모양: <code>{ id, rules, ruleIds, source, filesInScope, zeroAdmissionRules? }[]</code>, <code>id</code> 로 정렬되며, 팩마다의 룰 개수와 출처를 함께 싣는다(<code>source</code>: <code>"dir"</code> = <code>packsDir</code> 디렉터리에서 읽음, <code>"inline"</code> = <code>packDefs</code>). <code>ruleIds</code> 는 <code>rules</code> 라는 <em>개수</em> 뒤에 있는 <em>목록</em> — 이 실행이 그 팩에서 보고할 수 있었던 룰 id 전부라, "이 실행에 X 라는 룰이 있나"를 팩 접두사로 추측하지 않고 답장에서 바로 답할 수 있다. 항상 실려 있다(빠져 있으면 "이 빌드는 말하기를 거부한다"로 읽히는데, 그건 "그런 룰 없다"와 반드시 구분돼야 하는 상태다).`,
      en: `Shape: <code>{ id, rules, ruleIds, source, filesInScope, zeroAdmissionRules? }[]</code>, sorted by <code>id</code>, with each pack's rule count and provenance (<code>source</code>: <code>"dir"</code> = read from a <code>packsDir</code> directory, <code>"inline"</code> = <code>packDefs</code>). <code>ruleIds</code> is the LIST behind the <code>rules</code> COUNT — every rule id this run could have reported from that pack, so "does this run carry a rule named X" is answered from the reply instead of guessed from the pack prefix. Always present: an omitted list would read as "this build declines to say", which is exactly the state a validator has to tell apart from "no such rule".`,
    },

    p138: {
      ko: `<code>filesInScope</code> 는 그 팩의 룰들이 경로상 스캔할 자격이 있는 파일 수를 센다(<code>file_pattern</code> 후보 자격이고, 내용 검사 이전이다) — <code>filesInScope &gt; 0</code> 인데 발견이 0 이면 "돌았고 아무것도 없었다"로, <code>filesInScope: 0</code> 이면 "범위에 든 것이 없다"로 읽어라. <code>zeroAdmissionRules</code> 는 같은 센서스를 룰 단위로 낸 것이다: 자기 경로 관문(<code>file_pattern</code> 과 그 룰의 <code>file_exclude_pattern</code>)이 여기서 파일을 하나도 들이지 않는 이 팩의 룰 id 들이고, 그들의 발견 0 은 "검사했고 깨끗하다"가 아니라 범위 문제다. 비어 있지 않을 때만 있고, 팩 수준의 0 이 이미 모든 룰을 덮는 <code>filesInScope: 0</code> 팩에서는 빠진다.`,
      en: `<code>filesInScope</code> counts the files a pack's rules are path-eligible to scan (<code>file_pattern</code> candidacy, before any content check) — pair <code>filesInScope &gt; 0</code> with zero findings to read "ran, found nothing" versus <code>filesInScope: 0</code> "nothing in scope." <code>zeroAdmissionRules</code> is the same census per rule: the ids of this pack's rules whose own path gates (<code>file_pattern</code> plus that rule's <code>file_exclude_pattern</code>) admit zero files here — their zero findings are scope, not "checked and clean". Present only when non-empty; omitted on a <code>filesInScope: 0</code> pack, whose pack-level zero already covers every rule.`,
    },

    p139: {
      ko: `언제나 있다 — <code>[]</code> 가 "DSL 팩을 0 개 적재했다"는 정직한 상태다. 커스텀 팩이 실제로 적재됐는지를 발견 개수의 차이로 미루어 짐작하지 않고 확인해 준다.`,
      en: `Always present — <code>[]</code> is the honest "zero DSL packs loaded" state. Verifies a custom pack actually loaded without inferring it from findings deltas.`,
    },

    summary140: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>ruleOverridesApplied</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">룰 손잡이 셋이 적용됐다는 긍정 확인.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>ruleOverridesApplied</code><span class="cmd-row__type">object</span></span><span class="cmd-row__desc">Positive confirmation that the three rule knobs were applied.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p141: {
      ko: `모양: <code>{ disabled, severityRemapped, only }</code> — <code>disabledRules</code>/<code>severityOverrides</code>/<code>packsOnly</code> 가 적용됐다는 것과 영향을 받은 룰 id 들을 싣고, <code>only</code> 는 존중된 팩 허용 목록(<code>packs.only</code>)으로 그 바깥의 팩은 돌지 않았다는 뜻이다. 셋 중 아무것도 요청되지 않았으면 빠지거나 비어 있다 — 없는 키는 <code>null</code> 이 아니라 "오버라이드 없음"으로 읽어라.`,
      en: `Shape: <code>{ disabled, severityRemapped, only }</code> — that <code>disabledRules</code>/<code>severityOverrides</code>/<code>packsOnly</code> were applied, listing the affected rule ids, with <code>only</code> being the honored pack allowlist (<code>packs.only</code>), outside which a pack never ran. Omitted (or empty) when none of the three was requested — treat an absent key as "no overrides," never <code>null</code>.`,
    },

    summary142: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>configWarnings</code><span class="cmd-row__type">string[]</span></span><span class="cmd-row__desc">설정을 쓰다 난 문제. <code>warnings</code> 바깥에 따로 둔다. 언제나 있다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>configWarnings</code><span class="cmd-row__type">string[]</span></span><span class="cmd-row__desc">Config-authoring problems, kept OUT of <code>warnings</code>. Always present.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p143: {
      ko: `분석 시점에 계산된다: 알려진 어떤 룰 id 와도 맞지 않는 <code>disabledRules</code>/<code>severityOverrides</code> 항목이 여기 보고된다(알려진 id 전체 집합을 가진 것은 분석 시점뿐이다). <code>[]</code> 는 두 손잡이 어느 쪽에도 아무것도 맞히지 못한 항목이 없었다는 뜻이다.`,
      en: `Computed at analysis time: a <code>disabledRules</code>/<code>severityOverrides</code> entry matching no known rule id is reported here (only analysis time has the full known-id set). <code>[]</code> means neither knob had a matching-nothing entry.`,
    },

    summary144: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>ruleTimings</code><span class="cmd-row__type">object[] | null</span></span><span class="cmd-row__desc">프로파일링이 켜져 있을 때, 룰 id + 걸린 시간 + 발견 개수.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>ruleTimings</code><span class="cmd-row__type">object[] | null</span></span><span class="cmd-row__desc">Per-rule id + elapsed time + finding count, when profiling is enabled.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p145: {
      ko: `채우려면 요청에 <code>profileRules: true</code> 를 세워라(CLI 방언: <code>zzop analyze --profile-rules</code> / <code>zzop analyze-envelope --profile-rules</code> / <code>zzop cross --profile-rules</code>). 프로파일링이 꺼져 있으면 — 그게 기본값이다 — <code>null</code> 이다. <code>zzop.config.jsonc</code> 키가 없고(타이밍 보고는 프로젝트에 대한 사실이 아니라 한 기계에서의 한 번의 호출에 대한 질문이다), 이것을 켜는 MCP 도구 인자도 없으므로 오늘 <code>analyze_repo</code> 응답은 타이밍을 싣지 않는다.`,
      en: `Set <code>profileRules: true</code> on the request to populate it (CLI dialect: <code>zzop analyze --profile-rules</code> / <code>zzop analyze-envelope --profile-rules</code> / <code>zzop cross --profile-rules</code>); <code>null</code> when profiling was off, which is the default. It has no <code>zzop.config.jsonc</code> key — a timing report is a question about one invocation on one machine, not a fact about the project — and no MCP tool argument turns it on, so an <code>analyze_repo</code> reply carries no timings today.`,
    },

    p146: {
      ko: `<code>EnvelopeAnalyzeRequest</code> 도 같은 필드를 싣는다: 모드 A 의 팩 평가(파일별 symbol-scan, 트리 전체 io-scan)와 그 전체 그래프 분석이 네이티브 경로가 쓰는 것과 같은 타이밍 누산기로 흘러든다. 프로파일링은 <code>findings</code>/<code>ir</code> 를 바꾸지 않고 캐시 키에도 끼지 않는다. 캐시에서 통째로 나온 파일은 어떤 파일 단위 룰도 다시 돌리지 않아 타이밍에 기여하지 않으므로, <strong>따뜻한</strong> 실행은 전체 그래프 네이티브 분석만 보고한다.`,
      en: `<code>EnvelopeAnalyzeRequest</code> carries the same field: Mode A's pack evaluation (symbol-scan per file, io-scan whole-tree) and its whole-graph analyses feed the same timing accumulator the native path uses. Profiling never changes <code>findings</code>/<code>ir</code> and takes no part in the cache key; a file served whole from cache re-runs no per-file rule and contributes no timing, so a WARM run reports only the whole-graph native analyses.`,
    },

    summary147: {
      ko: `<div class="cmd-row"><span class="cmd-row__name"><code>disclosure</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">실행 전역의 침묵 실패 계열 등기부 — 정적이고 실행마다 동일하다.<span class="cmd-row__more">계약</span></span></div>`,
      en: `<div class="cmd-row"><span class="cmd-row__name"><code>disclosure</code><span class="cmd-row__type">object[]</span></span><span class="cmd-row__desc">Run-global silent-failure-class registry — static and identical every run.<span class="cmd-row__more">contract</span></span></div>`,
    },

    p148: {
      ko: `어떤 계열의 실명을 zzop 이 탐지하고 어떤 것은 아직 탐지하지 못하는지에 대한 정직한 목록이다. 다중 트리 <code>analyzeTrees</code> 호출에서는 트리마다가 아니라 <code>trees</code> 옆에 한 번만 앉는다. 세 필드의 정의는 아래 <a href="#disclosure-registry">등기부 범례</a>에 있다.`,
      en: `zzop's honest list of which classes of blindness it does and does not yet detect. On a multi-tree <code>analyzeTrees</code> call it sits once beside <code>trees</code>, never per tree. Its three fields are defined in <a href="#disclosure-registry">the registry legend</a> below.`,
    },

    p149: {
      ko: `<strong>이 색인은 파사드 와이어</strong>이고, 배열 전체를 싣는다 — 파생의 출처다. 정형된 제품 응답(<code>zzop analyze</code>/<code>cross</code>/<code>endpoint</code> 와 그 MCP 쌍둥이)은 2026-07-29 부터 대신 <strong>접힌 것</strong>을 싣는다: 개수와 포인터(<code>zzop contract disclosure-classes</code> / <code>zzop://contract/disclosure-classes</code>). 그 산문은 실행에 따라 변하지 않으므로 호출마다가 아니라 한 번만 배포된다.`,
      en: `<strong>This index is the FACADE wire</strong>, which carries the full array — it is the derivation source. The shaped product replies (<code>zzop analyze</code>/<code>cross</code>/<code>endpoint</code> and their MCP twins) carry a <strong>fold</strong> instead since 2026-07-29: the counts plus a pointer (<code>zzop contract disclosure-classes</code> / <code>zzop://contract/disclosure-classes</code>). The prose is run-invariant, so it ships once rather than on every call.`,
    },

    p150: {
      ko: `JSON 트리는 위에서 아래까지 전부 camelCase 다 — 최상위 뷰뿐 아니라 중첩된 모든 타입이 자기 표기 규칙을 함께 지닌다. <code>Finding.data</code> 가 의도된 단 하나의 예외다: 균일한 표기 규칙이 없는, 룰 작성자가 쓴 불투명한 JSON 이다.`,
      en: `The whole JSON tree is camelCase, top to bottom — every nested type carries its own casing rule, not just the top-level view. <code>Finding.data</code> is the one deliberate exception: opaque, rule-authored JSON with no uniform casing rule.`,
    },

    p151: {
      ko: `<strong>severity 가 주장하는 것.</strong> severity 는 그 발견 하나에 대한 zzop 의 확신을 말하고, 그 확신은 이번 실행이 볼 수 있었던 것에 묶여 있다 — zzop 은 소스를 읽지, 돌아가는 시스템을 찔러 보지 않는다. 크로스레이어 룰 둘이 스스로 등급을 낮춤으로써 그 경계를 눈에 보이게 만든다: <code>cross-layer/unconsumed-mutation-endpoint</code> 와 <code>cross-layer/unprovided-mutation-call</code> 은, HTTP 호출이 대부분 풀리지 않은 채 돌아온 소스를(각각, 서버 프레임워크를 import 하면서도 라우트를 거의 내놓지 못한 소스를) 실행이 들고 있을 때 <code>warning</code> 이 아니라 <code>info</code> 로 보고하고 그 소스를 메시지에서 이름 댄다. 어느 쪽이든 발견은 발화하므로, 이것은 억제가 아니라 확신의 눈금 맞추기다. 그 역은 <strong>성립하지 않고</strong>, warning 갈래의 메시지가 스스로 그렇게 말한다: 실명 검사 하나하나는 좁은 술어 하나이므로, 룰이 <code>warning</code> 에 머물렀다는 것은 실명이 <em>목격되지 않았다</em>는 뜻이지 커버리지가 완전하다고 증명됐다는 뜻이 아니다. 이 추출이 모델링하지 않는 호출 모양이나 언어로 부르는 호출자, 또는 이번 실행 바깥의 레포지토리에 있는 호출자는, 룰에게 그런 것과 똑같이 이 검사에게도 보이지 않는다. severity 를 판결로 대하기 전에, 이번 실행이 자기 한계를 스스로 적은 <code>warnings</code> 와 아래 <code>coverage</code> 센서스를 읽어라.`,
      en: `<strong>What a severity asserts.</strong> Severity states zzop's confidence in that one finding, and that confidence is bounded by what this run could see — zzop reads source and never probes a running system. Two cross-layer rules make the bound visible by de-escalating themselves: <code>cross-layer/unconsumed-mutation-endpoint</code> and <code>cross-layer/unprovided-mutation-call</code> report at <code>info</code> rather than <code>warning</code> when the run holds a source whose HTTP calls came back mostly unresolved (respectively, a source that imports a server framework yet yielded almost no routes), naming that source in the message; the finding fires either way, so this is a confidence match and not suppression. The converse does <strong>not</strong> follow, and the warning-branch message says so itself: each blindness check is one narrow predicate, so a rule holding at <code>warning</code> means blindness was <em>not witnessed</em> — not that coverage was proven complete. A caller in a call shape or language this extraction does not model, or in a repository outside the run, is invisible to the check exactly as it is to the rule. Read <code>warnings</code> and the <code>coverage</code> census below for the run's own account of its limits before treating a severity as a verdict.`,
    },

    h2152: {
      ko: `좁아진 범위는 말없이가 아니라 warnings 에 스스로 보고한다`,
      en: `A narrowed scope self-reports in warnings, never silently`,
    },

    p153: {
      ko: `에러는 호출이 실패했다는 뜻이다. 능력이 없는 것은 에러가 아니다 — 분석은 정상적으로 끝나고, 엔진이 무엇을 건너뛰었으며 왜 그랬는지를 정확히 말한다. 이 자기보고는 JS 래퍼가 아니라 엔진 자신 안에서 일어나므로, Rust 엔진을 직접 부르는 JS 아닌 소비자에게도 똑같이 적용된다.`,
      en: `An error means the call failed. A missing capability is not an error — the analysis completes normally, and the engine says exactly what it skipped and why. This self-report happens inside the engine itself, not the JS wrapper, so it applies identically to a non-JS consumer calling the Rust engine directly.`,
    },

    p154: {
      ko: `DSL 룰팩을 하나도 찾지 못했을 때도 같은 방식이다: 엔진은 네이티브 분석만 돌았다는 사실과 그 개수를 이름 대지, 설명 없이 조용히 더 작아진 <code>findings</code> 배열을 돌려주지 않는다.`,
      en: `The same pattern applies when no DSL rule packs could be found: the engine reports that only its native analyses ran, and names how many, rather than returning a quietly smaller <code>findings</code> array with no explanation.`,
    },

    h2155: {
      ko: `프로세스는 죽지 않는다`,
      en: `The process never crashes`,
    },

    p156: {
      ko: `<code>crates/facade/src/lib.rs</code>(크레이트 <code>zzop-facade</code>)는 계약상 결코 패닉하지 않는다 — 실패할 수 있는 모든 경로(망가진 JSON, 빠진 <code>root</code>, 유효하지 않은 엔벨로프)는 대신 <code>Result&lt;String, String&gt;</code> 을 돌려준다. 엔진은 파일 하나의 파싱/룰 실패를 이미 내부에서 격리하고, 그것이 바깥 경계에 닿기 한참 전에 그렇게 한다. Rust 로 직접 부르는 쪽은 <code>Ok(String)</code> 아니면 <code>Err(String)</code> 만 보고, 프로세스 중단은 결코 보지 않는다. <code>zzop-mcp</code> 바이너리는 사이에 FFI 경계 없이 이 함수들을 부르므로, 따로 따져 볼 애드온 쪽 <code>catch_unwind</code> 층 같은 것은 없다 — 파사드 자신의 계약이 이야기의 전부다. <code>version_string</code> 에는 <code>Result</code> 자체가 없다 — 실패할 수 없다.`,
      en: `<code>crates/facade/src/lib.rs</code> (crate <code>zzop-facade</code>) never panics by contract — every fallible path (malformed JSON, a missing <code>root</code>, an invalid envelope) returns a <code>Result&lt;String, String&gt;</code> instead. The engine already isolates a single file's parse/rule failure internally, well before it would ever reach that outer boundary. A direct Rust caller sees either <code>Ok(String)</code> or <code>Err(String)</code>, never a process abort; the <code>zzop-mcp</code> binary calls these functions with no FFI boundary in between, so there is no separate addon-side <code>catch_unwind</code> layer to reason about — the facade's own contract is the whole story. <code>version_string</code> has no <code>Result</code> at all — it cannot fail.`,
    },

  },
};
