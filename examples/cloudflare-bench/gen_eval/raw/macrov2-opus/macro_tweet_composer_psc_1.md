<!-- exp=macrov2-opus task=macro_tweet_composer cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.079454 -->
```python
"use client"
R*

seed_tweets = [
    {"id": 1, "author": "@ada", "text": "Hello, world of tweets!", "likes": 3},
    {"id": 2, "author": "@grace", "text": "Compilers are just very patient friends.", "likes": 7},
    {"id": 3, "author": "@alan", "text": "Can machines think? Asking for a feed.", "likes": 42},
]

MAX_LEN = 280


@c
def TweetFeed():
    tweets, set_tweets = us(seed_tweets)
    text, set_text = us("")

    remaining = MAX_LEN - len(text)
    over = remaining < 0
    empty = len(text.strip()) == 0

    def post():
        if empty or over:
            return
        new_id = (max([t["id"] for t in tweets]) if tweets else 0) + 1
        new_tweet = {"id": new_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t
                    for t in tweets])

    counter_class = "char-counter over" if over else "char-counter"

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
            ),
            div(cn="composer-bar",
                span(cn=counter_class, f"{remaining}"),
                button(
                    cn="post-button",
                    oc=lambda: post(),
                    dis=empty or over,
                    "Post",
                ),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda: like(t["id"]),
                    f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
