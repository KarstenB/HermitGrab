function fgrep --wraps='rg -F' --description 'alias fgrep=rg -F'
  rg -F $argv
        
end
