<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.0545826 -->
```python
"use client"
R*

SEED_TWEETS = [
    {"id": 1, "author": "ada", "text": "Shipped the analytical engine today.", "likes": 12},
    {"id": 2, "author": "grace", "text": "Compilers are just very patient translators.", "likes": 8},
    {"id": 3, "author": "linus", "text": "Talk is cheap. Show me the code.", "likes": 41},
]

MAX_LEN = 280

@c
def TweetFeed():
    tweets, set_tweets = us(SEED_TWEETS)
    draft, set_draft = us("")

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": (tweets[0]["id"] + 1) if len(tweets) > 0 else 1,
            "author": "you",
            "text": draft.strip(),
            "likes": 0,
        }
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    def mk_like(tweet_id):
        return lambda: like(tweet_id)

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=draft,
                oh=lambda e: set_draft(e.target.value),
                ph="What's happening?",
            ),
            div(cn="composer-footer",
                span(cn="char-counter" if remaining >= 0 else "char-counter char-counter-over",
                     f"{remaining}"),
                button(cn="post-button", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(cn="tweet", key=t["id"],
                div(cn="tweet-author", f"@{t['author']}"),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=mk_like(t["id"]),
                    f"♥ {t['likes']}"),
            )
            for t in tweets
        ]),
    )
```
