update: update-github-colors update-github-api

update-github-colors:
    curl https://raw.githubusercontent.com/ozh/github-colors/refs/heads/master/colors.json \
    > src/colors.json

update-github-api:
    # See https://docs.github.com/en/graphql/overview/public-schema
    curl https://docs.github.com/public/fpt/schema.docs.graphql \
    > src/api/github/schema.docs.graphql
