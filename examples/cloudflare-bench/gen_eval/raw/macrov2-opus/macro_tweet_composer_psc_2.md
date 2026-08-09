<!-- exp=macrov2-opus task=macro_tweet_composer cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.030213999999999998 -->
```python
"use client"
R*

SEED = [
    {"id": 3, "author": "@ada", "text": "just shipped a new compiler pass", "likes": 12},
    {"id": 2, "author": "@grace", "text": "found a bug. it was a real moth.", "likes": 48},
    {"id": 1, "author": "@alan", "text": "can machines think?", "likes": 99},
]

LIMIT = 280

@c
def TweetFeed():
    tweets, set_tweets = us(SEED)
    text, set_text = us("")
    next_id, set_next_id = us(4)

    remaining = LIMIT - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        tweet = {"id": next_id, "author": "@me", "text": text.strip(), "likes": 0}
        set_tweets([tweet, *tweets])
        set_next_id(next_id + 1)
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t
                    for t in tweets])

    return div(cn="tweet-feed",
        div(cn="compose",
            textarea(cn="compose-input", value=text, ph="What's happening?",
                     oh=lambda e: set_text(e.target.value)),
            div(cn="compose-bar",
                span(cn="counter" if remaining >= 0 else "counter over", remaining),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Tweet"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda t=t: like(t["id"]),
                    f"♥ {t['likes']}"),
            ) for t in tweets]),
    )
```
