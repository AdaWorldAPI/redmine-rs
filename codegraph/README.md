# Click paths of Redmine, read from its Rails source

These files say what Redmine lets a user reach and who may reach it, read from the
Rails source by `rails-codegraph` (in AdaWorldAPI/zammad-rs). Nothing here is
written by hand. The configuration is `redmine.toml`. No code in the tool names
Redmine.

| File | What it holds |
| --- | --- |
| `harvest/klickwege-redmine.json` | Per stratum: declarations read, how many became rows, how many escaped |
| `harvest/access-redmine.json` | Controller filters (`before_action` and the like) with their `only:` and `except:` lists |
| `harvest/frontend-routes-redmine.json` | Frontend routes (empty: none are configured) |
| `harvest/seed-rows-redmine.json` | Seed rows (empty: none are configured) |
| `harvest/constants-redmine.json` | Literal class constants (empty: none are configured) |
| `harvest/slag-klickwege-redmine.json` | Everything that was not read, each with its reason and remedy: the work queue |

Each stratum balances: declarations = rows + escaped.

## Regenerate

From a zammad-rs checkout, with the Redmine source checked out:

```bash
REDMINE_SRC=/path/to/redmine cargo run -p rails-codegraph --bin rails-codegraph -- /path/to/this/repo/codegraph/redmine.toml --klickwege
```

The tool writes into `harvest/` beside the configuration.
