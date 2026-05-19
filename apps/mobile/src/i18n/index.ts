/**
 * PhoneMic 移动端 i18n —— 字典加载、语言决策与运行时切换。
 *
 * 任务来源：tasks.md 9.22（i18n 字典与运行时切换 + decideLang）
 * 关联需求：R8.2、R8.3、R8.5、R8.6
 * 设计来源：design.md §4.7
 *
 * 模块对外暴露：
 *  - {@link UiLang}：UI 语言枚举（与桌面端 `Lang` 同义）；
 *  - {@link decideLang}：把任意 locale 字符串映射为 {@link UiLang}（纯函数，与
 *    桌面端 `crates/phonemic-core/src/i18n.rs::decide_lang` 行为一致）；
 *  - {@link dictFor}：取得目标语言字典（对应导入的 JSON 字面量）；
 *  - {@link translate}：按 key 查文案，缺失时控制台告警并回退到 `en-US`；
 *  - {@link DICTS}：所有字典原始对象（暴露用于属性测试 / 完整性检查）。
 *
 * 与桌面端字典的关系：
 *  桌面端字典 `crates/phonemic-core/src/i18n_dict/{zh-CN,en-US}.json` 仅承载
 *  桌面端 UI 文案，移动端字典 keys 是其超集，包含手机界面专属字段。两份
 *  字典之间的对齐由 9.25 的字典完整性单元测试守护。
 */

import zhCNRaw from './dicts/zh-CN.json'
import enUSRaw from './dicts/en-US.json'

/** 受支持的 UI 语言；与桌面端 `Lang` 一对一映射。 */
export type UiLang = 'zh-CN' | 'en-US'

/** UI 语言常量数组（用于属性测试枚举与穷尽性检查）。 */
export const UI_LANGS: readonly UiLang[] = ['zh-CN', 'en-US'] as const

/**
 * i18n 字典：key → 文案的映射。
 *
 * 选择 `Record<string, string>` 而非更窄的 `Record<DictKey, string>` 是因为
 * 字典可能在前端独立扩展 key，桌面端不一定同步；运行时缺失通过
 * {@link translate} 的回退路径处理。
 */
export type Dict = Record<string, string>

/** 字典常量集合：编译期内嵌，避免运行期 fetch。 */
export const DICTS: Readonly<Record<UiLang, Dict>> = Object.freeze({
  'zh-CN': zhCNRaw as Dict,
  'en-US': enUSRaw as Dict,
})

/**
 * 把任意 locale 字符串决策为 UI 语言。
 *
 * 规则（与桌面端 `decide_lang` 完全一致）：
 *  1. 先 trim；空串 / 纯空白 → `en-US`；
 *  2. 取首段（按 `-` 或 `_` 拆分）作为主语言子标签；
 *  3. 主子标签忽略大小写等于 `"zh"` → `zh-CN`，其它一律 `en-US`。
 *
 * 该函数为纯函数，无 IO、无副作用、确定性。
 */
export function decideLang(locale: string | null | undefined): UiLang {
  if (locale == null) return 'en-US'
  const trimmed = locale.trim()
  if (trimmed.length === 0) return 'en-US'
  // 取首段：按 `-` 或 `_` 拆分；BCP-47 用 `-`，POSIX (`zh_CN.UTF-8`) 用 `_`。
  const idxDash = trimmed.indexOf('-')
  const idxUnder = trimmed.indexOf('_')
  let endIdx: number
  if (idxDash === -1 && idxUnder === -1) {
    endIdx = trimmed.length
  } else if (idxDash === -1) {
    endIdx = idxUnder
  } else if (idxUnder === -1) {
    endIdx = idxDash
  } else {
    endIdx = Math.min(idxDash, idxUnder)
  }
  const primary = trimmed.slice(0, endIdx)
  return primary.toLowerCase() === 'zh' ? 'zh-CN' : 'en-US'
}

/** 取得 `lang` 对应的字典对象（直接返回引用，调用方不应修改）。 */
export function dictFor(lang: UiLang): Dict {
  return DICTS[lang]
}

/**
 * 占位符替换：把 `"foo {x} bar"` + `{ x: 1 }` 渲染为 `"foo 1 bar"`。
 *
 * 这是 i18n 标准做法之一，比模板字符串更安全（缺少变量时原样保留 token）。
 */
function format(template: string, vars?: Readonly<Record<string, string | number>>): string {
  if (!vars) return template
  return template.replace(/\{(\w+)\}/g, (match, key: string) => {
    const v = vars[key]
    return v === undefined ? match : String(v)
  })
}

/**
 * 按 key 取目标语言文案；缺失时回退到 `en-US`，再缺失则返回 key 本身并控制台告警。
 *
 * 这是 9.25 描述的"缺失 key 在控制台告警并回退到 en-US"行为的运行时实现。
 */
export function translate(
  lang: UiLang,
  key: string,
  vars?: Readonly<Record<string, string | number>>,
): string {
  const dict = DICTS[lang]
  const direct = dict[key]
  if (direct !== undefined) return format(direct, vars)
  // 回退到英文。
  const fallback = DICTS['en-US'][key]
  if (fallback !== undefined) {
    // eslint-disable-next-line no-console
    console.warn(`[i18n] missing key "${key}" in dict "${lang}", falling back to en-US`)
    return format(fallback, vars)
  }
  // eslint-disable-next-line no-console
  console.warn(`[i18n] missing key "${key}" in all dictionaries`)
  return key
}
