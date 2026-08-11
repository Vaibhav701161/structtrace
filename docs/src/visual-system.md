# Visual system

StructTrace uses an evidence-led visual language rather than a generic AI dashboard aesthetic.
The source of truth is `ui/public/assets/structtrace-design-tokens.json`; the embedded application
and documented assets must remain synchronized with it.

## Palette roles

| Role | Light | Dark | Meaning |
| --- | --- | --- | --- |
| Canvas | `#F1EEE8` | `#181A17` | Warm neutral workspace |
| Surface | `#FAF8F3` | `#20231F` | Primary evidence panels |
| Text | `#242521` | `#F3EFE6` | Graphite / warm-white hierarchy |
| Selection | `#95502F` | `#D09268` | StructTrace identity and active selection |
| Verified | `#176B55` | `#63B89B` | Passed checks and verified integrity only |
| Regression | `#A53E35` | `#E2786D` | Failures, regressions, and blocked authority |
| Warning | `#815713` | `#D2A55C` | Insufficient evidence and caution |
| Evidence | `#4F6668` | `#91AAA9` | Non-authorizing information |

Selection color must not imply success. Verified, warning, and regression colors are reserved for
semantic state. Charts use direct labels, explicit denominators, shared axes, and textual marks so
that meaning does not depend on color alone. Decorative gradients and glow effects are prohibited.

Evidence views use 13 px minimum monospace text, sticky JSON Pointer controls, visible
added/removed/changed/type-changed markers, and exact numeric lexemes. Light and dark themes share
the same information hierarchy and focus treatment. Any token change requires contrast and
responsive browser verification before release.
