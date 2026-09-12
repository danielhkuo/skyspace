# AGENTS.md

Rules for AI agents working in this repository.
These rules override default agent behaviour. They apply to every agent, every session, every task.

---

## 1. Never commit

**Committing is forbidden. Only a human commits in this repository.**

| Allowed | Forbidden |
|---|---|
| `git add` | `git commit` |
| `git status`, `git diff`, `git log` | `git push` |
| `git stash list`, `git show` | `git merge`, `git rebase` |
| Reading any file | `git reset --hard`, `git clean` |
| Creating and editing files | `git tag`, `git revert` |
| | Creating pull requests |
| | `git config` changes |

### The required workflow

1. Make the file changes.
2. Stage them: `git add <paths>`.
3. Report what changed.
4. Print a proposed commit message in a code block.
5. Stop. The human reviews the staged diff and runs the commit.

### If you are asked to commit

Do not run the command. State this rule, print the commit message, and let the human run it.
This applies even when the task looks finished, the change looks small, or a commit seems convenient.

### Why

Every change must pass human review before it enters history. A commit made by an agent skips that review.

---

## 2. Report in simplified technical English

Write so a reader who is tired, or who reads English as a second language, understands you on the first pass.

| Rule | Do | Do not |
|---|---|---|
| Sentence length | One idea per sentence | Long sentences joined by commas |
| Vocabulary | Common words | Idioms, metaphors, slang |
| Order | Result first, then detail | Build-up before the answer |
| Lists | Table or bullets for more than three items | A paragraph holding a list |
| Names | Exact file, endpoint and parameter names | "the config", "that endpoint" |
| Tone | Plain statements | Praise, filler, marketing words |

More rules:

- Define a term the first time you use it.
- Say what failed. Do not hide errors inside a summary.
- Mark anything you did not verify. Write "unverified" or "not tested".
- Do not claim a task is done unless you ran it and saw the result.
- Give numbers when you have them. "4,978 sections" beats "many sections".

Example.

Bad: "Great news — the scraper is humming along nicely and the data looks fantastic!"
Good: "The scraper finished. It pulled 4,978 sections from 96 subjects. No requests failed."

---

## 3. Project rules

**Data sources.**

- Use only public Rice pages: `courses.rice.edu` and `ga.rice.edu`.
- Never scrape Esther. Never request, store, or type a NetID password.
- Space requests at least 150 ms apart. Cache responses.
- Identify the client in the User-Agent, with a real contact address.

**Data handling.**

- Do not commit pulled data. Catalog and requirement output is in `.gitignore`.
- Requirement extraction from `ga.rice.edu` produces a **draft**. A person who knows the program must review it before it affects a student's plan.
- Never present extracted or scraped data as verified. Say which page you fetched and when.

**Product rules.**

- The plan is the student's document. The application warns. It never blocks.
- Any rule the engine cannot verify becomes a visible self-check, never a silent omission.

---

## 4. Before you finish a task

Check each item:

- [ ] Changes are staged with `git add`.
- [ ] Nothing was committed, pushed, merged, or rebased.
- [ ] A proposed commit message is printed in the reply.
- [ ] The report uses simplified technical English.
- [ ] Failures and unverified claims are stated plainly.
