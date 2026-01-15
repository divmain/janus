# Slack Notify Recipe

Send Slack notifications when important ticket events occur in Janus.

## What it does

This recipe notifies your Slack channel when:

- A new ticket is created
- A ticket is marked as complete
- A new plan is created

## Setup

### 1. Create a Slack Incoming Webhook

1. Go to [Slack API Apps](https://api.slack.com/apps)
2. Create a new app (or select an existing one)
3. Enable **Incoming Webhooks** under Features
4. Click **Add New Webhook to Workspace**
5. Select the channel where you want notifications
6. Copy the webhook URL (it looks like `https://hooks.slack.com/services/T.../B.../...`)

### 2. Set the Environment Variable

Add your webhook URL to your shell profile (`.bashrc`, `.zshrc`, etc.):

```bash
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
```

### 3. Install the Recipe

```bash
janus hook install slack-notify
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `SLACK_WEBHOOK_URL` | Yes | Your Slack incoming webhook URL |

If `SLACK_WEBHOOK_URL` is not set, the hook exits silently without error.

## Example Messages

### New Ticket Created
```
New ticket created: j-a1b2 - Implement user authentication
```
Displayed with a blue accent bar.

### Ticket Completed
```
Ticket completed: j-a1b2
```
Displayed with a green accent bar.

### New Plan Created
```
New plan created: plan-c3d4 - Q1 Roadmap
```
Displayed with a blue accent bar.

## Customization

To customize the messages or add more events, edit `.janus/hooks/slack-notify.sh` after installation.

Available events you can add:
- `ticket_deleted` - When a ticket is deleted
- `plan_updated` - When a plan is modified
- `plan_deleted` - When a plan is deleted
