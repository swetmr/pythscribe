<!-- exp=baseline-001 task=macro_tweet_composer cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.033181 -->
```python
"use client"
R*

LIMIT = 280

SEED = [
    {"id": 1, "author": "@ada", "text": "Compilers are just very opinionated calculators.", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Shipped a bug fix at 2am. The moth thanks you.", "likes": 47},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the diff.", "likes": 89},
]


def next_id(tweets):
    return (max([t["id"] for t in tweets]) if tweets else 0) + 1


@c
def TweetFeed():
    tweets, set_tweets = us(SEED)
    text, set_text = us("")

    remaining = LIMIT - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        tweet = {"id": next_id(tweets), "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([tweet, *tweets])
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
                maxlength=LIMIT,
            ),
            div(cn="composer-bar",
                span(cn="char-count" if remaining >= 0 else "char-count over", str(remaining)),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Tweet"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                span(cn="tweet-author", t["author"]),
                p(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda tid=t["id"]: like(tid), f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
