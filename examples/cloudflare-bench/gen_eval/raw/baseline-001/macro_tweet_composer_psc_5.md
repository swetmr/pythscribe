<!-- exp=baseline-001 task=macro_tweet_composer cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.031656000000000004 -->
```python
"use client"
R*

def counter_class(remaining):
    if remaining < 0:
        return "counter over"
    if remaining <= 20:
        return "counter low"
    return "counter"

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "@ada", "text": "Loving the new feed layout!", "likes": 4},
        {"id": 2, "author": "@grace", "text": "Shipping code on a Tuesday.", "likes": 12},
        {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 87},
    ])
    text, set_text = us("")

    remaining = 280 - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        next_id = (max([t["id"] for t in tweets]) if tweets else 0) + 1
        new_tweet = {"id": next_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
                maxLength=280,
            ),
            div(cn="composer-bar",
                span(cn=counter_class(remaining), f"{remaining}"),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
