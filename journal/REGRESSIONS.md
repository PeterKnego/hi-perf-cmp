# Regressions Registry

Confirmed performance regressions and how to avoid reintroducing them. This is
institutional memory — `journal compare` *detects* regressions; this file
*remembers* the confirmed ones.

Add an entry when a regression is confirmed (not for transient cloud-variance
noise). Newest first.

## Template

```
## YYYY-MM-DD — <short title>
- cell(s): <focus_area>/<experiment>/<language>/<metric> (+ others)
- magnitude: <e.g. p50 +35% vs run <id>>
- introduced by: <commit / change>
- root cause: <what actually caused it>
- fix: <commit / how it was resolved, or "open">
- guard: <how to avoid repeating it — a check, a note, a test>
```

---

_No regressions recorded yet._
