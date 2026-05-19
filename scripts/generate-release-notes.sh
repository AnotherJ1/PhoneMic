#!/usr/bin/env bash
# PhoneMic 发版说明草稿 —— 任务 14.5
#
# 任务来源：tasks.md 14.5
# 设计来源：design.md §9.7
#
# 从上一个 tag 起的 commit log 生成 markdown：
#   - 按 conventional-commits 类别分组 (feat / fix / docs / refactor / test / chore / 其它)
#   - 末尾附完整 git diff 链接
#
# stdout 即 markdown 内容；CI 把它重定向到 RELEASE_NOTES.md。

set -euo pipefail

CURRENT_TAG="${GITHUB_REF_NAME:-$(git describe --tags --abbrev=0 2>/dev/null || echo HEAD)}"
PREV_TAG="$(git describe --tags --abbrev=0 "${CURRENT_TAG}^" 2>/dev/null || true)"

if [[ -n "$PREV_TAG" ]]; then
  RANGE="${PREV_TAG}..${CURRENT_TAG}"
else
  RANGE="${CURRENT_TAG}"
fi

emit_section() {
  local title="$1" pattern="$2"
  local entries
  entries=$(git log --pretty='%h %s' "$RANGE" 2>/dev/null | grep -E "$pattern" || true)
  if [[ -n "$entries" ]]; then
    printf '\n### %s\n\n' "$title"
    while IFS= read -r line; do
      [[ -z "$line" ]] && continue
      printf -- '- %s\n' "$line"
    done <<< "$entries"
  fi
}

printf '# PhoneMic %s\n' "$CURRENT_TAG"
printf '\nReleased: %s\n' "$(date -u '+%Y-%m-%d %H:%M UTC')"
if [[ -n "$PREV_TAG" ]]; then
  printf '\nChanges since [%s]: `%s`.\n' "$PREV_TAG" "$RANGE"
fi

emit_section 'Features'      '^[a-f0-9]+ feat(\(|:|! )'
emit_section 'Bug fixes'     '^[a-f0-9]+ fix(\(|:|! )'
emit_section 'Documentation' '^[a-f0-9]+ docs(\(|:|! )'
emit_section 'Refactoring'   '^[a-f0-9]+ refactor(\(|:|! )'
emit_section 'Tests'         '^[a-f0-9]+ test(\(|:|! )'
emit_section 'Chores'        '^[a-f0-9]+ chore(\(|:|! )'

# Catch-all for commits that do not follow conventional-commits.
OTHERS=$(git log --pretty='%h %s' "$RANGE" 2>/dev/null \
  | grep -Ev '^[a-f0-9]+ (feat|fix|docs|refactor|test|chore)(\(|:|! )' || true)
if [[ -n "$OTHERS" ]]; then
  printf '\n### Other changes\n\n'
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    printf -- '- %s\n' "$line"
  done <<< "$OTHERS"
fi

if [[ -n "$PREV_TAG" ]]; then
  printf '\n---\nFull diff: `git log %s`\n' "$RANGE"
fi
