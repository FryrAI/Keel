# keel, minimum 16 characters

Source `911cca1748edaa8bb0056c50283ad1e41731060b`. Seed 87; sample size 1 / 1. Verdicts intentionally empty.

| Export index (0-based) | Homes | Literal location | Segment | Decoded literal | Verdict |
| --- | --- | --- | --- | --- | --- |
| 0 | counts_line (crates/keel-enforce/src/review/render.rs:98) | crates/keel-output/src/human.rs:393 | file(s) changed, | Checkpoint ({}): {} file(s) changed, {} error(s), {} warning(s)<br> | FALSE — Generic count wording shared by two different reports (the checkpoint header vs the review counts line, with different fields); calling counts_line would be wrong. |
