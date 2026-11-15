#!/bin/sh
# A pull request's title is a Conventional Commits subject (CONTRIBUTING.md,
# "Commit subjects"): a squash merge makes it the commit's subject.
# Usage: tools/pr-title.sh "<title>"
title=$1
types='feat|fix|perf|refactor|docs|test|build|ci|chore|style|revert'
if printf '%s' "$title" | grep -Eq "^($types)(\([a-z0-9._/-]+\))?!?: [^ ].*[^ ]$"; then
  echo "ok: $title"
else
  echo "::error::The title is not a Conventional Commits subject: $title"
  echo "Expected: <type>(<scope>)!: <what changed>, with <type> one of"
  echo "$(printf '%s' "$types" | sed 's/|/, /g'); the scope and the ! (a breaking change) are optional."
  echo "Example: feat(views): the list sorts by any column"
  exit 1
fi
