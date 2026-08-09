<!-- exp=baseline-001 task=macro_tweet_composer cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.029856 -->
```python
"use client"
R*

def initial_tweets():
    return [
        {"id": 1, "author": "@ada", "text": "Just shipped a new compiler pass.", "likes": 4},
        {"id": 2, "author": "@grace", "text": "Debugging is twice as hard as writing the code.", "likes": 12},
        {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 27},
    ]

LIMIT = 280

@c
def TweetFeed():
    tweets, set_tweets = us(initial_tweets())
    text, set_text = us("")
    next_id, set_next_id = us(4)

    remaining = LIMIT - len(text)
    over = remaining < 0
    empty = len(text.strip()) == 0

    def post():
        if empty or over:
            return
        new_tweet = {"id": next_id, "author": "@you", "text": text, "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_next_id(next_id + 1)
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="compose",
            textarea(
                cn="compose-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
            ),
            div(cn="compose-bar",
                span(cn="over-limit" if over else "char-count", str(remaining)),
                button(
                    cn="post-btn",
                    oc=lambda: post(),
                    dis=empty or over,
                    "Tweet",
                ),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                span(cn="tweet-author", t["author"]),
                p(cn="tweet-text", t["text"]),
                button(
                    cn="like-btn",
                    oc=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```
