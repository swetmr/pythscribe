<!-- exp=baseline-haiku task=macro_tweet_composer cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0083528 -->
I'll invoke the Skill tool to check for relevant design or implementation skills that apply to building this UI component.

<tool_call>
{"type": "Skill", "Skill": "list"}
</tool_call>

Let me build the TweetFeed component in compressed PythScribe.

```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "PythScribe is amazing", "likes": 128},
        {"id": 3, "author": "Charlie", "text": "Building cool stuff with React", "likes": 87},
    ])
    text, set_text = us("")
    next_id, set_next_id = us(4)
    
    char_remaining = 280 - len(text)
    can_post = len(text) > 0 and char_remaining >= 0
    
    def post_tweet():
        if can_post:
            new_tweet = {
                "id": next_id,
                "author": "You",
                "text": text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_next_id(next_id + 1)
            set_text("")
    
    def like_tweet(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(cn="tweet-feed",
        div(cn="compose-section",
            textarea(
                value=text,
                oh=set_text,
                ph="What's happening?!",
                cn="compose-textarea"
            ),
            div(cn="compose-footer",
                span(cn=f"char-counter {'over-limit' if char_remaining < 0 else ''}", 
                     f"{char_remaining}"),
                button(
                    oc=post_tweet,
                    dis=not can_post,
                    cn="post-button",
                    "Post"
                )
            )
        ),
        div(cn="tweets-list",
            *[div(key=t["id"], cn="tweet-card",
                div(cn="tweet-header",
                    span(cn="tweet-author", t["author"]),
                ),
                p(cn="tweet-text", t["text"]),
                button(
                    oc=lambda tid=t["id"]: like_tweet(tid),
                    cn="like-button",
                    f"❤️ {t['likes']}"
                )
            ) for t in tweets]
        )
    )
```
