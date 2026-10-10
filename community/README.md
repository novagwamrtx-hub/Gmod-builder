# Vouches

The two files here are written by a workflow, not by hand.

- `ledger.json` holds every verdict that has been shared, by GitHub account number: `true` vouches for a Workshop item, `false` warns about it. An account has one verdict per item, however often it shares.
- `scores.json` holds the totals per Workshop item, made from the ledger. The app downloads it when it starts.

## How a verdict gets here

1. In the app, someone vouches for addons or warns about them, then presses **Share** under Settings → Vouches.
2. Their browser opens a filled-in issue titled `Vouches` on this repository, and they press **Create**.
3. [`vouches.yml`](../.github/workflows/vouches.yml) counts every open issue with that title into the ledger, rewrites the totals, answers the issue and closes it.

The body of such an issue looks like this:

```
gmcb-vouches 1
up 104482086,160250458
down 266579667
undo 123456789
```

`up` vouches, `down` warns, and `undo` takes an earlier verdict back. The numbers are Workshop ids.

## Taking yours back

Take the verdict back in the app (click **✔ Vouched** or **Warned** again) and share: what is sent then includes the `undo`. To have everything an account shared removed, open an issue and ask.

## What is public

The ledger shows which account numbers said what, and anyone can look up the account behind a number. Treat a shared vouch as public, like a star on GitHub.

## What a score is worth

A score needs three accounts, and counts for more the more accounts are behind it. That keeps one person from making one, not a group that sets out to. If a score looks wrong, open an issue.
