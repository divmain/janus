# Time Tracking Hook Recipe

Track time spent on tickets by logging status transitions to a CSV file.

## What It Does

This recipe monitors status changes on tickets and tracks the wall-clock time spent in the `in_progress` status. When a ticket transitions:

- **TO `in_progress`**: Records the current timestamp as the start time
- **FROM `in_progress`**: Calculates the duration and logs it to a CSV file

## Installation

```bash
janus hook install time-tracking
```

## How Time Is Calculated

Time tracking uses wall-clock duration:

1. When a ticket status changes to `in_progress`, the current UTC timestamp is stored in `.janus/.time-tracking/<ticket-id>`
2. When the ticket status changes from `in_progress` to any other status, the duration is calculated as `(current_time - start_time)` in minutes
3. The transition is logged to `.janus/time-log.csv`

## CSV Output Format

The time log is written to `.janus/time-log.csv` with the following columns:

| Column | Description |
|--------|-------------|
| `timestamp` | UTC timestamp of the status change (ISO 8601) |
| `ticket_id` | The ticket identifier |
| `old_status` | Previous status (`in_progress`) |
| `new_status` | New status |
| `duration_minutes` | Time spent in `in_progress` (integer minutes) |

Example:
```csv
timestamp,ticket_id,old_status,new_status,duration_minutes
2024-01-15T10:30:00Z,j-a1b2,in_progress,complete,45
2024-01-15T14:00:00Z,j-c3d4,in_progress,next,120
```

## Analyzing the Time Log

### View total time per ticket

```bash
awk -F',' 'NR>1 && $5!="" {time[$2]+=$5} END {for(t in time) print t, time[t], "minutes"}' .janus/time-log.csv
```

### View total time spent across all tickets

```bash
awk -F',' 'NR>1 && $5!="" {sum+=$5} END {print sum, "minutes"}' .janus/time-log.csv
```

### View time spent per day

```bash
awk -F',' 'NR>1 && $5!="" {day=substr($1,1,10); time[day]+=$5} END {for(d in time) print d, time[d], "minutes"}' .janus/time-log.csv | sort
```

### Find tickets with most time spent

```bash
awk -F',' 'NR>1 && $5!="" {time[$2]+=$5} END {for(t in time) print time[t], t}' .janus/time-log.csv | sort -rn | head -10
```

### Export to a spreadsheet

The CSV format is compatible with Excel, Google Sheets, and other spreadsheet applications. Simply open the file directly or import it.

## Files Created

- `.janus/time-log.csv` - The time tracking log
- `.janus/.time-tracking/` - Directory containing start time files for in-progress tickets

## Limitations

- **Wall-clock time only**: Measures elapsed time, not active work time. If you set a ticket to `in_progress` and go to lunch, that time is counted.
- **Manual status changes**: Only tracks time between manual `janus status` commands. Does not integrate with external time tracking tools.
- **Single session**: If you restart work on a ticket (set to `in_progress` again), it starts a new timing session. Previous sessions are preserved in the log.
- **No pause/resume**: There's no way to pause timing without changing the status. Use `next` status to pause and `in_progress` to resume.
- **UTC timestamps**: All times are recorded in UTC. Convert as needed for reporting.
