# SDK maintenance operations

The weekly workflow runs Mondays at 09:00 America/Los_Angeles. It assesses one
focused provider contract or compatible fix using `.maintainer.yml`, validates
any patch independently, and opens or updates a PR. It does not merge or release.

## Verify a change before rollout

1. Update the pinned repo-maintainer commit in the weekly, diagnostic and
   completion workflows together. Review the shared change before merging.
2. Run the full pipeline without publishing:

   ```sh
   gh workflow run sdk-maintenance.yml --ref YOUR_REVIEW_BRANCH -f dry_run=true
   ```

   Dry runs read the review branch's configuration and assess the default branch
   or an existing maintenance PR. They may call the configured free-tier model;
   paid inference remains disabled unless explicitly enabled. `unchanged` is a
   valid successful assessment and skips patch validation/publication.
3. Check the Actions run and download `repository-maintenance-proposal` plus
   `maintenance-diagnostics-<job>-<attempt>`. A changed proposal must pass the
   independent validation job. A blocked proposal is a failure, not a successful
   maintenance pass.
4. Merge the shared maintainer change and this integration, then run
   `gh workflow run sdk-maintenance.yml` to verify normal publication behavior.
   Check the resulting PR/CI or the scoped no-change report before relying on
   the next scheduled run. Keep the old local schedule paused to avoid duplicates.

## Find a failure

The separate **Maintenance completion** workflow observes every completed weekly
run, even if the weekly workflow failed before starting a job. Its summary and
`maintenance-run-<id>` artifact record the revision, result, job count and failed
steps. A zero-job failure points to workflow inputs, references, syntax or
permissions; there cannot be agent logs in that case.

Once merged, an older run can be inspected with:

```sh
gh workflow run maintenance-completion.yml -f run_id=40400321483
```

For failures after startup, JSONL diagnostic artifacts record stage start/end,
elapsed time, run/attempt/job and redacted errors. They are retained for 30 days.
An unfinished `started` event suggests interruption or a timeout. Build failures
are in the ordinary Actions step logs. Review diagnostics before sharing because
errors may contain repository paths or content.

The normal maintainer still updates the existing **SDK maintenance needs help**
issue for actionable stage failures and closes it after recovery. The completion
observer adds a failed Actions check and report; it does not send duplicate issues.

Scheduled events do not have manual inputs. Keep boolean inputs explicitly typed
with expressions such as `inputs.allow_paid == true`; see the
[GitHub inputs context reference](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#inputs-context).
