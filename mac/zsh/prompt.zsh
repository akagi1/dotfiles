# Lightweight Git prompt: + staged, ! unstaged, ? untracked.
autoload -Uz add-zsh-hook

_dotfiles_git_prompt() {
  local status_output line branch flags='' staged='' unstaged='' untracked=''
  local -a status_lines
  typeset -g _dotfiles_git_info=''

  status_output=$(GIT_OPTIONAL_LOCKS=0 command git status --porcelain=v1 --branch \
    --untracked-files=normal --ignore-submodules=dirty 2>/dev/null) || return 0
  status_lines=("${(@f)status_output}")
  branch=${status_lines[1]#\#\# }
  branch=${branch%%...*}
  branch=${branch#No commits yet on }
  branch=${branch#Initial commit on }
  [[ $branch == 'HEAD (no branch)' ]] && branch='detached'

  for line in "${status_lines[@]:1}"; do
    if [[ ${line[1,2]} == '??' ]]; then
      untracked='?'
    else
      [[ ${line[1]} != ' ' ]] && staged='+'
      [[ ${line[2]} != ' ' ]] && unstaged='!'
    fi
  done
  flags=${staged}${unstaged}${untracked}
  [[ -n $flags ]] && flags=" $flags"
  # Escape percent signs because zsh interprets them during prompt rendering.
  branch=${branch//\%/%%}
  _dotfiles_git_info=" %F{yellow}[$branch$flags]%f"
}

add-zsh-hook -d precmd _dotfiles_git_prompt
add-zsh-hook precmd _dotfiles_git_prompt
setopt prompt_subst
PROMPT='%F{green}%B%~%b%f${_dotfiles_git_info} %F{green}%B%#%b%f '
