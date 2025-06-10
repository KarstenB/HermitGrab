function egrep --wraps='rg -E' --description 'alias egrep=rg -E'
  rg -E $argv
        
end
