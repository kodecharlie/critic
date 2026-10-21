# Instructions for setting up `critic`
`critic` comprises several software components that need to be installed separately:

1. `PostgreSQL`, which functions as our vector database.
2. `ollama`, which manages our models.
3. `nomic-embed-text`, an open-source model we use for archiving user-reviews as embeddings in our PostgreSQL instance.
4. `mistral`, an open-source model we use for querying details about user-reviews.
5. CLI command in Rust.

Below, we walk through the setup for each of these components.  We assume a macOS environment.  You will have to adjust to your environment accordingly.

## PostgreSQL
We recommend using `brew` to install `PostgreSQL`.
```
brew install postgres
```
For purposes of supporting embeddings, you will need to enable the `pgvector` extension for PostgreSQL:
```
CREATE EXTENSION IF NOT EXISTS vector;
```

## ollama
Run:
```
brew install ollama
ollama serve
```

## nomic-embed-text
Run:
```
ollama pull nomic-embed-text
```

## mistral
Run:
```
ollama pull mistral
```

# Configuration
Write the file `${HOME}/.criticrc`:
```
[apple]
apple.api.host=developer.apple.com
apple.api.key=XXX

[jira]
jira.host=192.168.1.251
jira.apikey=YYY

# Data store for housing embeddings of reviews.
[embeddings]
embeddings.host=localhost
embeddings.user=critic
embeddings.password=critic123
```
Adjust these settings per your environment.

# Initialization
To initialize `critic` for an iOS named `PocketSort`, run this command:
```
critic init PocketSort
```
This commands pulls user-reviews for `PocketSort` from Apple, scrubs them, and then populates them as embeddings in the vector database in PostgreSQL.

## Updates
For subsequent updates -- that is, to refresh your embeddings with the latest user-reviews posted for your app -- run a command like this:
```
critic update PocketSort
```