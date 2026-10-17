#!/bin/bash

codex_directory="${HOME}/.codex"
chatgpt_cache_directories=(
    "${HOME}/Library/Caches/com.openai.chat"
    "${HOME}/Library/Caches/com.openai.codex"
    "${HOME}/Library/Caches/ChatGPTHelper"
    "${HOME}/Library/Containers/com.openai.chat/Data/Library/Caches/com.openai.chat"
    "${HOME}/Library/Containers/com.openai.codex/Data/Library/Caches/com.openai.codex"
)

echo "Codex directory: ${codex_directory}"
echo "This will permanently delete:"
echo "  sessions"
echo "  archived_sessions"
echo "  generated_images"
echo "  session_index.jsonl"
echo "  history.jsonl"
echo "  state_5.sqlite"
echo "  state_5.sqlite-shm"
echo "  state_5.sqlite-wal"
echo "ChatGPT cache directories found:"
has_data=0
for cache_directory in "${chatgpt_cache_directories[@]}"; do
    if [ -d "${cache_directory}" ]; then
        echo "  ${cache_directory}"
        has_data=1
    fi
done
echo "ChatGPT login and settings will be preserved."
echo "Close Codex and ChatGPT before continuing."

if [ -d "${codex_directory}" ]; then
    has_data=1
else
    echo "No Codex directory was found. Only ChatGPT caches will be considered."
fi

if [ "${has_data}" -eq 0 ]; then
    echo "No Codex directory or supported ChatGPT cache directories were found. Nothing to clean."
    read -r -p "Press Enter to close..."
    exit 0
fi

printf "Permanently delete the listed Codex data and ChatGPT caches? [y/N] "
read -r confirmation

case "${confirmation}" in
    y|Y)
        ;;
    *)
        echo "Operation cancelled."
        read -r -p "Press Enter to close..."
        exit 0
        ;;
esac

cleanup_failed=0

rm -rf "${codex_directory}/sessions" || cleanup_failed=1
rm -rf "${codex_directory}/archived_sessions" || cleanup_failed=1
rm -rf "${codex_directory}/generated_images" || cleanup_failed=1
rm -f "${codex_directory}/session_index.jsonl" || cleanup_failed=1
rm -f "${codex_directory}/history.jsonl" || cleanup_failed=1
rm -f "${codex_directory}/state_5.sqlite" || cleanup_failed=1
rm -f "${codex_directory}/state_5.sqlite-shm" || cleanup_failed=1
rm -f "${codex_directory}/state_5.sqlite-wal" || cleanup_failed=1

for cache_directory in "${chatgpt_cache_directories[@]}"; do
    if [ -d "${cache_directory}" ]; then
        rm -rf "${cache_directory}" || cleanup_failed=1
    fi
done

if [ "${cleanup_failed}" -ne 0 ]; then
    echo "Error: one or more Codex data files or ChatGPT cache directories could not be removed."
    read -r -p "Press Enter to close..."
    exit 1
fi

echo "The listed Codex data and ChatGPT caches were cleaned successfully."
read -r -p "Press Enter to close..."
