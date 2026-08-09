<!-- exp=baseline-sonnet task=macro_tweet_composer cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.072054 -->
```python
"use client"
R*

MAX = 280

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 3, "author": "ada", "text": "Shipping a new PythScribe demo today.", "likes": 12},
        {"id": 2, "author": "grace", "text": "Compilers are just very opinionated translators.", "likes": 8},
        {"id": 1, "author": "linus", "text": "Merged. Ship it.", "likes": 41},
    ])
    draft, set_draft = us("")

    remaining = MAX - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": len(tweets) + 1, "author": "you", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(cn="composer-input", value=draft, ph="What's happening?",
                      oh=lambda e: set_draft(e.target.value)),
            div(cn="composer-footer",
                span(cn="char-count-over" if remaining < 0 else "char-count", f"{remaining}"),
                button(cn="post-button", dis=not can_post, oc=lambda: post(), "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda: like(t["id"]), f"♥ {t['likes']}"),
            )
            for t in tweets
        ]),
    )
```
