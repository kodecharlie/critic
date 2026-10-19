# What is `critic`?
`critic` is a CLI tool that provides Apple developers visibility into what reviewers are saying about their apps.  It leverages modern AI techniques such as LLMs and vector databases for purposes of surfacing critical insights from users of published apps.

# How it works
`critic` operates as a two-phased operation:

1. It encodes a data pipeline that readies user-reviews for analysis.
2. It facilitates CLI interactions with the AI-ready reviews.

## Data pipeline
`critic` readies user-reviews for analysis by way of the following data pipeline:

1. Query secure API endpoints from Apple to download app-specific user-reviews.
2. Scrub said user-reviews of any spurious characters or PPI data.
3. Archive said user-reviews as embeddings in a vector database.

It is up to you to decide how often to execute the data pipeline.  Once run, subsequent runs should merely augment existing embeddings (in the vector database) with whatever additional user-reviews have been posted to the App Store since the most recent execution of the data pipeline.  We recommend running the data pipeline once every 30-90 days -- unless there are urgent needs on your end for more up-to-date analyes of user-reviews.  We expect the incremental updates to take a few minutes at most.

## CLI interactions
Once the data pipeline is complete, `critic` can then be executed from the CLI for purposes of probing user-reviews for answers to simple questions:

1. What is working for users, and what is not?
2. What bugs should be addressed?  And what is the urgency of fixes for those bugs?
3. How many reviews are users posting over time, and with what frequency?
4. Is user-sentiment about the app improving or decling over time?
5. Can I cull bugs from reviews and automatically post them to my bug-tracking system?

# How it is executed
`critic` is encoded as a Rust application and is intended to be run from the CLI (for macOS environments, that usually means a shell in the `Terminal` app).  `critic`, like numerous other CLI utilities, relies on an extensible catalog of sub-commands and options to capture different facets of reviews that users have written about iOS apps.  We spell out sub-commands below.

## Configuration encoded in INI file
`critic` loads its configuration from `${HOME}/.critic`, where `${HOME}` is your shell home directory.  The configuration is formatted as an `INI` file, and should look like this:
```
# See --bugtracker option for *bugs* sub-command.
[jira]
host=192.168.1.251
user=critic
password=critic123

# Data store for housing embeddings of reviews.
[embeddings]
host=localhost
user=critic
password=critic123
```

## `critic summary`
For the named app, print a summary of all reviews to date.  By default, the summary comprises an aggregate star-rating, which is a weighted average over all ratings; the top five most favorable observations; the bottom five least favorable observations; and a few statistics like number of ratings, and average sentiment weighted by average star-rating.

### Basic usage
From the CLI, if you wish to query the review history for an app named `PocketSort`, run this command:
```
critic summary PocketSort
```

### Bound `summary` by calendar dates
To confine your analysis to a specific date-range, run this command:
```
critic summary PocketSort --begin 2021-01-01 --end 2025-12-31
```
where the actual cutoff for the boundaries is midnight, or 12:00.000 AM GMT, for the begin-date, and 11:59.999 PM GMT for the end-date.

## `critic bugs`
Query the LLM assembled from reviews for defects users have identified in the named app.  The intent of this sub-command is to produce coherent bug descriptions (from reviews) which in turn could be used to report new issues in a bug-tracking system.

### Basic usage
From the CLI, if you wish to surface bugs for an app named `PocketSort`, run this command:
```
critic bugs PocketSort
```
which orders bugs over all time in descending order of severity, where severity is calculated as a cross-product of frequency of issue-references in reviews times proximity to the current date/time (where proximity should be in [0,1] and reflects the time-distance of the review from the current date/time).

### Bound `bugs` by calendar dates
To confine your analysis to a specific date-range, run this command:
```
critic bugs PocketSort --begin 2021-01-01 --end 2025-12-31
```
where the actual cutoff for the boundaries is midnight, or 12:00.000 AM GMT, for the begin-date, and 11:59.999 PM GMT for the end-date.

### Options for `bugs` sub-command
Other options for the `bugs` sub-command include:

* `--json` ... which outputs a JSON glob listing the key attributes of each bug (such as description, severity, priority).
* `--bugtracker [jira|gnats|bugzero|mantis]` ... which attempts to cross-reference a listed bug with its status in a bug-tracking system, and then indicates that status next to the bug printed in the terminal console.  The bugtracker specified would need to be listed as a section in the configuration for `critic`.

## `critic counts`
Query the timestamps of posted reviews and render a time-series to the terminal that indicates the number of reviews posted per unit-time.  By default, the unit-time will be a quarter or three months.  The time-series will be rendered vertically across the terminal console -- where an asterisk will represent a unit-count.  At the end of each horizontal bar (of asterisks) will be a number equal to the total count for that point in time.

### Basic usage
From the CLI, if you wish to query the review history for an app named `PocketSort`, run this command:
```
critic counts PocketSort
```

### Bound `counts` by calendar dates
To confine your analysis to a specific date-range, run this command:
```
critic counts PocketSort --begin 2021-01-01 --end 2025-12-31
```
where the actual cutoff for the boundaries is midnight, or 12:00.000 AM GMT, for the begin-date, and 11:59.999 PM GMT for the end-date.
