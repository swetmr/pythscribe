<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0241086 -->
```python
"use client"
R*

def make_id():
    return str(int(Date.now() * 1000 + Math.random() * 1000))

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": "1", "author": "ada", "text": "shipped the compiler today", "likes": 12},
        {"id": "2", "author": "grace", "text": "compilers are just very opinionated translators", "likes": 5},
        {"id": "3", "author": "linus", "text": "it compiles, ship it", "likes": 42},
    ])
    draft, set_draft = us("")

    limit = 280
    remaining = limit - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": make_id(), "author": "you", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=draft,
                oh=lambda e: set_draft(e.target.value),
                ph="What's happening?",
            ),
            div(cn="composer-footer",
                span(cn="char-count" if remaining >= 0 else "char-count char-count-over",
                     f"{remaining}"),
                button(oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda: like(t["id"]),
                       f"♥ {t['likes']}"),
            )
            for t in tweets
        ]),
    )
```
