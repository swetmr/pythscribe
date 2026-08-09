<!-- exp=macrov2-opus task=macro_tweet_composer cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.028388999999999998 -->
```python
"use client"
R*

SEED = [
    {"id": 1, "author": "@ada", "text": "Hello from the feed!", "likes": 3},
    {"id": 2, "author": "@grace", "text": "Shipping PythScribe today.", "likes": 7},
    {"id": 3, "author": "@linus", "text": "It compiles. Ship it.", "likes": 12},
]

LIMIT = 280

@c
def TweetFeed():
    text, set_text = us("")
    tweets, set_tweets = us(SEED)

    remaining = LIMIT - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new = {"id": len(tweets) + 1, "author": "@you", "text": text, "likes": 0}
        set_tweets([new, *tweets])
        set_text("")

    def like(i):
        updated = [t for t in tweets]
        updated[i] = {**updated[i], "likes": updated[i]["likes"] + 1}
        set_tweets(updated)

    return div(cn="tweet-feed",
        div(cn="compose",
            textarea(
                cn="compose-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
            ),
            div(cn="compose-bar",
                span(cn="counter" if remaining >= 0 else "counter over",
                     f"{remaining}"),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Tweet"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                span(cn="tweet-author", t["author"]),
                p(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda: like(i),
                       f"♥ {t['likes']}"),
            ) for i, t in enumerate(tweets)],
        ),
    )
```
